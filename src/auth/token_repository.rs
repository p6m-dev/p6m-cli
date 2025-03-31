use crate::{App, AuthN, AuthToken, Client};
use anyhow::{Context, Result};
use atty::Stream;
use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, Duration, Local, Utc};
use jsonwebtokens::raw::{self, TokenSlices};
use log::{debug, trace};
use openid::AccessTokenResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    env,
    fmt::{self, Display, Formatter},
    fs,
};

use super::openid;

#[derive(Debug, Clone)]
pub enum TryReason {
    LoginCommand,
    SsoCommand,
    WhoAmICommand,
    LoginTo(App),
    RefreshFor(App),
}

impl Display for TryReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TryReason::LoginCommand => write!(f, "for `login` command"),
            TryReason::SsoCommand => write!(f, "for `sso` command"),
            TryReason::WhoAmICommand => write!(f, "for `whoami` command"),
            TryReason::LoginTo(source) => write!(f, "to {}", source.name),
            TryReason::RefreshFor(source) => write!(f, "for {}", source.name),
        }
    }
}

pub enum TryAuthReason {
    Login((TryReason, AuthReason)),
    Refresh((TryReason, AuthReason)),
}

impl Display for TryAuthReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TryAuthReason::Login((try_reason, auth_reason)) => {
                write!(f, "{auth_reason} credentials {try_reason}")
            }
            TryAuthReason::Refresh((try_reason, auth_reason)) => {
                write!(f, "{auth_reason} tokens {try_reason}")
            }
        }
    }
}

pub enum AuthReason {
    Forcing,
    Expired,
    Assertion,
}

impl Display for AuthReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AuthReason::Forcing => write!(f, "Forcing"),
            AuthReason::Expired => write!(f, "Expired"),
            AuthReason::Assertion => write!(f, "Assertion"),
        }
    }
}

/// Acts as an abstraction for reading and writing tokens from disk.
#[derive(Debug, Clone)]
pub struct TokenRepository {
    pub auth_n: AuthN,
    auth_dir: Utf8PathBuf,
    organization_id: Option<String>,
    force: bool,
    scopes: Vec<String>,
    default_scopes: String,
    desired_claims: Claims,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Claims {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    // Assertable/mergable claims
    #[serde(
        rename = "https://p6m.dev/v1/permission/login/kubernetes",
        skip_serializing_if = "Option::is_none"
    )]
    pub login_kubernetes: Option<String>,
    #[serde(
        rename = "https://p6m.dev/v1/orgs",
        skip_serializing_if = "Option::is_none"
    )]
    pub orgs: Option<BTreeMap<String, String>>,
    #[serde(
        rename = "https://p6m.dev/v1/org",
        skip_serializing_if = "Option::is_none"
    )]
    pub org: Option<String>,

    #[serde(
        rename = "https://p6m.dev/v1/permission",
        skip_serializing_if = "Option::is_none"
    )]
    pub permissions: Option<Vec<String>>,

    #[serde(
        rename = "https://p6m.dev/v1/roles",
        skip_serializing_if = "Option::is_none"
    )]
    pub roles: Option<Vec<String>>,
}

impl Display for Claims {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string(&self.clone())
                .context("unable to dsplay claims")
                .unwrap()
        )
    }
}

impl Claims {
    pub fn assert(&self, desired_claims: &Claims) -> Result<()> {
        debug!("asserting claims: {:?}", self);
        debug!("desired_claims: {:?}", desired_claims);

        let self_json = serde_json::to_value(self)?;
        let desired_json = serde_json::to_value(desired_claims)?;

        let self_map = self_json
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("self is not a JSON object"))?;
        let desired_map = desired_json
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("desired_claims is not a JSON object"))?;

        desired_map.iter()
            // Only check non-null desired values.
            .filter(|(_, v)| !v.is_null())
            .try_for_each(|(key, desired_value)| -> Result<()> {
                let self_value = self_map.get(key)
                    .ok_or_else(|| anyhow::anyhow!("Missing field: {}", key))?;
                match (desired_value, self_value) {
                    // Handle array matching.
                    (Value::Array(exp_arr), Value::Array(act_arr)) => match exp_arr.as_slice() {
                        // If expected array is empty, the actual array must also be empty.
                        [] if act_arr.is_empty() => Ok(()),
                        [] => Err(anyhow::anyhow!(
                            "Field {} mismatch: expected empty array, found {:?}", key, self_value
                        )),
                        // If expected array contains exactly one element "*" then actual array must be non-empty.
                        [Value::String(s)] if s == "*" => {
                            if act_arr.is_empty() {
                                Err(anyhow::anyhow!(
                                    "Field {} mismatch: expected non-empty array due to wildcard, found empty array", key
                                ))
                            } else {
                                Ok(())
                            }
                        },
                        // Otherwise, require an exact match.
                        _ if exp_arr == act_arr => Ok(()),
                        _ => Err(anyhow::anyhow!(
                            "Field {} mismatch: expected {:?}, found {:?}", key, desired_value, self_value
                        )),
                    },
                    // For non-array values, require exact equality.
                    _ if desired_value == self_value => Ok(()),
                    _ => Err(anyhow::anyhow!(
                        "Field {} mismatch: expected {:?}, found {:?}", key, desired_value, self_value
                    )),
                }
            })?;

        debug!("claims assertion passed");
        Ok(())
    }

    pub fn merge(&mut self, from: Claims) {
        let mut existing =
            serde_json::to_value(self.clone()).expect("Failed to serialize existing");
        let incoming = serde_json::to_value(from).expect("Failed to serialize incoming");

        if let (Value::Object(existing_map), Value::Object(incoming_map)) =
            (&mut existing, &incoming)
        {
            for (key, incoming_value) in incoming_map {
                if !incoming_value.is_null() {
                    existing_map.insert(key.clone(), incoming_value.clone());
                }
            }
        }

        *self = serde_json::from_value(existing).expect("Failed to deserialize merged");
    }
}

impl Into<Claims> for Option<String> {
    fn into(self) -> Claims {
        match self {
            None => Claims::default(),
            Some(token) => {
                let TokenSlices { claims, .. } = raw::split_token(&token)
                    .context("unable to split token")
                    .map_err(|e| {
                        debug!("unable to split token: {e}");
                        e
                    })
                    .unwrap_or(TokenSlices {
                        header: "",
                        claims: "",
                        signature: "",
                        message: "",
                    });

                serde_json::from_value::<Claims>(
                    raw::decode_json_token_slice(claims)
                        .context("unable to decode token")
                        .map_err(|e| {
                            debug!("unable to decode token: {e}");
                            e
                        })
                        .unwrap_or_default(),
                )
                .map_err(|e| {
                    debug!("unable to parse token: {e}");
                    e
                })
                .unwrap_or_default()
            }
        }
    }
}

impl TokenRepository {
    pub const DEFAULT_SCOPES: &str = "openid email offline_access login:cli";

    /// Creates a [TokenRepository] given a [P6mEnvironment].
    pub fn new(auth_n: &AuthN, auth_dir: &Utf8PathBuf) -> Result<Self> {
        fs::create_dir_all(&auth_dir)?;

        let mut token_repository = TokenRepository {
            auth_n: auth_n.clone(),
            auth_dir: auth_dir.clone(),
            organization_id: None,
            force: false,
            scopes: vec![],
            default_scopes: Self::DEFAULT_SCOPES.to_string(),
            desired_claims: Claims::default(),
        };

        token_repository
            .default_scopes
            .clone()
            .split(" ")
            .for_each(|scope| {
                token_repository.with_scope(scope, Claims::default());
            });

        Ok(token_repository)
    }

    pub fn force(&mut self) -> &mut Self {
        self.force = true;
        self
    }

    pub fn with_organization(&mut self, organization: &String) -> Result<&mut Self> {
        let token_repository = Self::new(&self.auth_n, &self.auth_dir)?;

        if !token_repository.is_logged_in() {
            return Err(anyhow::Error::msg(
                "Please run `p6m login` before acquiring an organization token.",
            ));
        }

        let id_claims = token_repository
            .read_claims(AuthToken::Id)?
            .context("unable to read claims from id token")?;

        let organization_id = id_claims
            .clone()
            .orgs
            .and_then(|orgs| {
                orgs.into_iter()
                    .find(|org| {
                        // match on either the key (org id) or the value (org name)
                        org.0 == organization.to_string() || org.1 == organization.to_string()
                    })
                    .map(|o| o.0)
            })
            .context("missing desired organization in access token claims")
            .map_err(|e| {
                debug!("Unable to find organization {organization} in claims: {id_claims}",);
                e
            })?;

        self.with_organization_id(&organization_id)?;
        self.with_scope(
            format!("org:{}", organization_id).as_str(),
            Claims {
                org: Some(organization.clone()),
                ..Default::default()
            },
        );

        Ok(self)
    }

    pub async fn with_authn_app_id(&mut self, id: &String) -> Result<&mut Self> {
        let app = Client::new(&self.auth_n.apps_uri())
            .with_token(self.read_token(AuthToken::Id)?)
            .app(id)
            .await
            .context("Unable to get app")?;

        self.with_app(&app).context("Unable to set app")?;

        match self
            .try_refresh(&TryReason::RefreshFor(app.clone()))
            .await
            .map_err(|e| {
                debug!("Unable to refresh: {}", e);
                e
            })
            .ok()
        {
            None => {
                // TODO
                debug!("Unable to refresh, trying to login");
                self.force()
                    .try_login(&&TryReason::LoginTo(app.clone()))
                    .await?;
            }
            _ => {}
        };

        Ok(self)
    }

    pub fn with_scope(&mut self, scope: &str, desired_claims: Claims) -> &mut Self {
        self.scopes.push(scope.to_string());
        self.scopes.sort();
        self.scopes.dedup();
        self.desired_claims.merge(desired_claims);
        self
    }

    pub async fn try_login(&mut self, reason: &TryReason) -> Result<&mut Self> {
        let access_token_response = match self.force {
            true => {
                self.clear()?;
                self.login(TryAuthReason::Login((reason.clone(), AuthReason::Forcing)))
                    .await?
            }
            false => match self.try_refresh(reason).await?.read_tokens().ok() {
                Some(access_token_response) => access_token_response,
                None => {
                    self.login(TryAuthReason::Login((reason.clone(), AuthReason::Expired)))
                        .await?
                }
            },
        };

        self.assert_claims(
            &access_token_response,
            TryAuthReason::Login((reason.clone(), AuthReason::Assertion)),
        )
        .await?;
        self.write_tokens(&access_token_response)?;

        Ok(self)
    }

    pub async fn try_refresh(&mut self, reason: &TryReason) -> Result<&mut Self> {
        let access_token_response = match (self.force, self.should_refresh()?) {
            (true, _) => {
                self.refresh(TryAuthReason::Refresh((
                    reason.clone(),
                    AuthReason::Forcing,
                )))
                .await?
            }
            (_, true) => match self
                .refresh(TryAuthReason::Refresh((
                    reason.clone(),
                    AuthReason::Expired,
                )))
                .await
            {
                Ok(access_token_response) => access_token_response,
                Err(_) => {
                    self.login(TryAuthReason::Login((reason.clone(), AuthReason::Expired)))
                        .await?
                }
            },
            _ => self.read_tokens()?,
        };

        self.assert_claims(
            &access_token_response,
            TryAuthReason::Refresh((reason.clone(), AuthReason::Assertion)),
        )
        .await?;
        self.write_tokens(&access_token_response)?;

        Ok(self)
    }

    async fn login(&mut self, reason: TryAuthReason) -> Result<AccessTokenResponse> {
        debug!("attempting login due to: {reason}");
        if atty::isnt(Stream::Stdin) {
            let cmd = env::args().into_iter().collect::<Vec<_>>().join(" ");
            return Err(anyhow::Error::msg(format!(
                "Please run `{cmd}` in an interactive session."
            )));
        }

        let device_code_request = openid::DeviceCodeRequest::new(self).await?;

        let access_token_response = device_code_request
            .login(&reason)
            .await
            .context("unable to exchange device code for tokens")
            .map_err(|e| {
                debug!("Unable to exchange device code for tokens: {e}");
                e
            })?;

        Ok(access_token_response)
    }

    async fn refresh(&mut self, reason: TryAuthReason) -> Result<AccessTokenResponse> {
        debug!("attempting refresh due to: {reason}");

        let refresh_token = self
            .read_token(AuthToken::Refresh)
            .context("unable to read refresh token")?
            .context("missing refresh token")
            .map_err(|e| {
                debug!("Unable to read refresh token: {e}");
                e
            })?;

        let mut device_code_request = openid::DeviceCodeRequest::new(self).await?;

        let access_token_response = device_code_request
            .refresh(&refresh_token)
            .await
            .context("unable to refresh tokens")
            .map_err(|e| {
                debug!("Unable to refresh tokens: {e}");
                e
            })?;

        Ok(access_token_response)
    }

    async fn assert_claims(
        &self,
        access_token_resonse: &AccessTokenResponse,
        reason: TryAuthReason,
    ) -> Result<()> {
        debug!("asserting claims due to: {reason}");
        let claims: Claims = Into::into(access_token_resonse.id_token.clone());
        claims.assert(&self.desired_claims).map_err(|e| {
            debug!("Claim assertion failed: {e}");
            e
        })
    }

    pub fn clear(&self) -> Result<()> {
        fs::remove_dir_all(&self.auth_dir)?;
        fs::create_dir_all(&self.auth_dir)?;
        Ok(())
    }

    /// Appends organization_id to the path for the stored tokens
    fn with_organization_id(&mut self, organization_id: &String) -> Result<()> {
        self.organization_id = Some(organization_id.clone());
        self.auth_dir = self.auth_dir.join(organization_id);
        fs::create_dir_all(&self.auth_dir)?;
        Ok(())
    }

    fn with_app(&mut self, app: &App) -> Result<()> {
        self.auth_n = app.auth_n.clone().context("missing authn")?;
        self.auth_dir = self.auth_dir.join(format!("app_{}", app.client_id));
        self.scopes = vec![];
        self.default_scopes = "".into();
        self.desired_claims = Claims::default();
        fs::create_dir_all(&self.auth_dir)?;
        Ok(())
    }

    /// Read a token from disk.
    ///
    /// Returns an [Ok] with [None] if the token does not exist,
    /// an [Ok] with [Some] if it exists and read successfully,
    /// or an [Err] if there was an error accessing the file.
    pub fn read_token(&self, token_type: AuthToken) -> Result<Option<String>> {
        let path = self.token_path(token_type);
        if !path.exists() {
            debug!("{path} does not exist");
            Ok(None)
        } else {
            trace!("Reading {path}");
            Ok(Some(fs::read_to_string(path)?))
        }
    }

    /// Reads claims on a token.
    pub fn read_claims(&self, token_type: AuthToken) -> Result<Option<Claims>> {
        let claims = match self
            .read_token(token_type)
            .context("missing token")?
            .clone()
        {
            Some(token) => {
                let TokenSlices { claims, .. } =
                    raw::split_token(&token).context("unable to split token")?;
                Some(
                    serde_json::from_value::<Claims>(
                        raw::decode_json_token_slice(claims).context("unable to decode token")?,
                    )
                    .context("unable to parse token")?,
                )
            }
            None => None,
        };

        debug!("Token claims: {:?}", claims);

        Ok(claims)
    }

    pub fn is_logged_in(&self) -> bool {
        let id_token = self.read_token(AuthToken::Id).unwrap_or(None);
        let access_token = self.read_token(AuthToken::Access).unwrap_or(None);
        let refresh_token = self.read_token(AuthToken::Refresh).unwrap_or(None);

        if id_token.is_none() || access_token.is_none() {
            return false;
        }

        return refresh_token.is_some();
    }

    pub fn should_refresh(&self) -> Result<bool> {
        trace!("Checking if tokens should be refreshed");

        let id_pre_exp = self.clone().read_expiration(AuthToken::Id)? - Duration::hours(1);
        let access_pre_exp = self.clone().read_expiration(AuthToken::Access)? - Duration::hours(1);

        let access_token_will_exp = Utc::now() > access_pre_exp;
        let id_token_will_exp = Utc::now() > id_pre_exp;

        debug!("Access token expiring? {access_token_will_exp}");
        debug!("Id token expiring? {id_token_will_exp}");

        Ok(access_token_will_exp || id_token_will_exp)
    }

    // Get the expiration date of the desired token
    pub fn read_expiration(self, token_type: AuthToken) -> Result<DateTime<Utc>> {
        let claims = self.read_claims(token_type.clone())?.unwrap_or_default();

        let exp = DateTime::from_timestamp(claims.exp.unwrap_or(Utc::now().timestamp()), 0)
            .context("unable to parse exp claim")?;

        debug!(
            "{token_type} expiration: {}",
            exp.with_timezone(&Local::now().timezone())
        );

        Ok(exp)
    }

    /// Write a token to disk.
    pub fn write_token(&self, token_type: AuthToken, token: Option<&String>) -> Result<()> {
        if let Some(token) = token {
            let path = self.token_path(token_type);
            trace!("Writing {path}");
            fs::write(path, token)?;
        }
        Ok(())
    }

    /// Write All Tokens that exist in the [AccessTokenResponse].
    pub fn write_tokens(&self, tokens: &AccessTokenResponse) -> Result<()> {
        self.write_token(AuthToken::Access, tokens.access_token.as_ref())?;
        self.write_token(AuthToken::Id, tokens.id_token.as_ref())?;
        self.write_token(AuthToken::Refresh, tokens.refresh_token.as_ref())?;
        Ok(())
    }

    pub fn read_tokens(&self) -> Result<AccessTokenResponse> {
        let access_token = self.read_token(AuthToken::Access)?.unwrap_or_default();
        let id_token = self.read_token(AuthToken::Id)?.unwrap_or_default();
        let refresh_token = self.read_token(AuthToken::Refresh)?.unwrap_or_default();

        Ok(AccessTokenResponse {
            access_token: Some(access_token),
            id_token: Some(id_token),
            refresh_token: Some(refresh_token),
            ..Default::default()
        })
    }

    /// The root directory where auth-related files are stored.
    pub fn auth_root(&self) -> &Utf8Path {
        self.auth_dir.as_path()
    }

    /// Creates a path to where a token should exist on disc corresponding to the [AuthToken]
    ///
    /// Created by joining the [Self::auth_root()] with the [AuthToken]'s [Display::to_string] method.
    fn token_path(&self, token_type: AuthToken) -> Utf8PathBuf {
        self.auth_root().join(token_type.to_string())
    }

    pub fn to_string(&self) -> String {
        if let Some(claims) = self.read_claims(AuthToken::Id).unwrap_or(None) {
            let detail: Vec<String> = match (
                claims.email.as_ref(),
                claims.org.as_ref(),
                claims.permissions.as_ref(),
            ) {
                (Some(email), Some(org), permissions) => {
                    vec![
                        format!("Email: {}", email),
                        format!("Organization: {}", org),
                        format!(
                            "Permissions: {}",
                            match permissions {
                                Some(permissions) => permissions.join(", "),
                                None => "None".to_string(),
                            }
                        ),
                    ]
                }
                (Some(email), None, _) => vec![format!("Email: {}", email)],
                _ => vec![],
            };

            return detail.join("\n");
        };

        "Not logged in".into()
    }

    pub fn to_json(&self) -> Result<String, anyhow::Error> {
        let claims = self
            .read_claims(AuthToken::Id)
            .context("unable to get claims")?
            .context("not logged in")?;
        Ok(serde_json::to_string_pretty(&claims)?)
    }

    pub async fn scope_str(&mut self) -> Result<String> {
        let existing_scopes: Vec<String> = self
            .read_claims(AuthToken::Access)
            .unwrap_or(Some(Claims::default()))
            .unwrap_or_default()
            .scope
            .unwrap_or(self.default_scopes.clone())
            .split(" ")
            .map(|s| s.to_string())
            .collect();

        for scope in existing_scopes {
            self.with_scope(scope.as_str(), Claims::default());
        }

        Ok(self.scopes.join(" "))
    }

    pub async fn acr_values_form_data(&mut self) -> Result<Vec<(String, String)>> {
        let mut form: Vec<(String, String)> = vec![];

        let mut acr_values: Vec<String> = vec![];

        if let Some(organization_id) = self.organization_id.clone() {
            acr_values.push(format!("urn:auth:acr:organization-id:{}", organization_id));
        }

        for scope in self.scope_str().await?.split(" ") {
            acr_values.push(format!("urn:auth:acr:scope:{}", scope));
        }

        acr_values.extend(self.auth_n.acr_values.clone().unwrap_or_default());

        if acr_values.len() > 0 {
            form.push(("acr_values".into(), acr_values.join(" ")));
        }

        Ok(form)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_array_match() {
        let actual = Claims {
            roles: Some(vec![]),
            ..Default::default()
        };
        let desired = Claims {
            roles: Some(vec![]),
            ..Default::default()
        };

        // Expected: Pass because both arrays are empty.
        assert!(actual.assert(&desired).is_ok());
    }

    #[test]
    fn test_empty_array_mismatch() {
        let actual = Claims {
            roles: Some(vec!["user".to_string()]),
            ..Default::default()
        };
        let desired = Claims {
            roles: Some(vec![]),
            ..Default::default()
        };

        // Expected: Fail because desired is empty but actual is not.
        assert!(actual.assert(&desired).is_err());
    }

    #[test]
    fn test_wildcard_array_match() {
        let actual = Claims {
            roles: Some(vec!["superadmins".to_string()]),
            ..Default::default()
        };
        let desired = Claims {
            roles: Some(vec!["*".to_string()]),
            ..Default::default()
        };

        // Expected: Pass because wildcard requires a non-empty array.
        assert!(actual.assert(&desired).is_ok());
    }

    #[test]
    fn test_wildcard_array_mismatch() {
        let actual = Claims {
            roles: Some(vec![]),
            ..Default::default()
        };
        let desired = Claims {
            roles: Some(vec!["*".to_string()]),
            ..Default::default()
        };

        // Expected: Fail because the actual array is empty.
        assert!(actual.assert(&desired).is_err());
    }

    #[test]
    fn test_exact_array_match() {
        let actual = Claims {
            roles: Some(vec!["admin".to_string(), "user".to_string()]),
            ..Default::default()
        };
        let desired = Claims {
            roles: Some(vec!["admin".to_string(), "user".to_string()]),
            ..Default::default()
        };

        // Expected: Pass because the arrays exactly match.
        assert!(actual.assert(&desired).is_ok());
    }

    #[test]
    fn test_exact_array_mismatch() {
        let actual = Claims {
            roles: Some(vec!["admin".to_string(), "user".to_string()]),
            ..Default::default()
        };
        let desired = Claims {
            roles: Some(vec!["user".to_string(), "admin".to_string()]), // order differs
            ..Default::default()
        };

        // Expected: Fail because ordering matters for exact equality.
        assert!(actual.assert(&desired).is_err());
    }

    #[test]
    fn test_non_array_field_match() {
        let actual = Claims {
            email: Some("test@example.com".to_string()),
            ..Default::default()
        };
        let desired = Claims {
            email: Some("test@example.com".to_string()),
            ..Default::default()
        };

        // Expected: Pass because the emails match.
        assert!(actual.assert(&desired).is_ok());
    }

    #[test]
    fn test_non_array_field_mismatch() {
        let actual = Claims {
            email: Some("actual@example.com".to_string()),
            ..Default::default()
        };
        let desired = Claims {
            email: Some("desired@example.com".to_string()),
            ..Default::default()
        };

        // Expected: Fail because the emails differ.
        assert!(actual.assert(&desired).is_err());
    }

    #[test]
    fn test_missing_field() {
        // For example, desired has login_kubernetes set but actual does not.
        let actual = Claims::default();
        let desired = Claims {
            login_kubernetes: Some("true".to_string()),
            ..Default::default()
        };

        // Expected: Fail because login_kubernetes is missing in actual.
        assert!(actual.assert(&desired).is_err());
    }

    #[test]
    fn test_merge_no_change_with_empty_incoming() {
        let mut original = Claims {
            email: Some("test@example.com".to_string()),
            login_kubernetes: Some("true".to_string()),
            ..Default::default()
        };
        // Incoming claims are all None, so nothing should change.
        let incoming = Claims::default();
        original.merge(incoming);
        assert_eq!(original.email, Some("test@example.com".to_string()));
        assert_eq!(original.login_kubernetes, Some("true".to_string()));
    }

    #[test]
    fn test_merge_overwrites_non_null() {
        let mut original = Claims {
            email: Some("old@example.com".to_string()),
            login_kubernetes: Some("false".to_string()),
            roles: Some(vec!["admin".to_string()]),
            ..Default::default()
        };

        let incoming = Claims {
            email: Some("new@example.com".to_string()),
            // login_kubernetes is None in incoming, so it should not override the original.
            login_kubernetes: None,
            roles: Some(vec!["superadmin".to_string()]),
            ..Default::default()
        };

        original.merge(incoming);
        assert_eq!(original.email, Some("new@example.com".to_string()));
        // The original login_kubernetes should remain unchanged.
        assert_eq!(original.login_kubernetes, Some("false".to_string()));
        // Roles should be overwritten.
        assert_eq!(original.roles, Some(vec!["superadmin".to_string()]));
    }

    #[test]
    fn test_merge_orgs_update() {
        use std::collections::BTreeMap;

        let mut original_orgs = BTreeMap::new();
        original_orgs.insert("org1".to_string(), "member".to_string());

        let mut incoming_orgs = BTreeMap::new();
        incoming_orgs.insert("org2".to_string(), "admin".to_string());

        let mut original = Claims {
            orgs: Some(original_orgs),
            ..Default::default()
        };

        let incoming = Claims {
            orgs: Some(incoming_orgs),
            ..Default::default()
        };

        original.merge(incoming);

        let mut expected = BTreeMap::new();
        expected.insert("org2".to_string(), "admin".to_string());
        // Expect that the entire orgs field is replaced by the incoming value.
        assert_eq!(original.orgs, Some(expected));
    }

    #[test]
    fn test_merge_partial_fields() {
        let mut original = Claims {
            email: Some("initial@example.com".to_string()),
            scope: Some("read".to_string()),
            org: Some("org1".to_string()),
            ..Default::default()
        };

        let incoming = Claims {
            scope: Some("write".to_string()),
            // Incoming org is None; original org should remain.
            org: None,
            ..Default::default()
        };

        original.merge(incoming);
        assert_eq!(original.email, Some("initial@example.com".to_string()));
        assert_eq!(original.scope, Some("write".to_string()));
        assert_eq!(original.org, Some("org1".to_string()));
    }
}
