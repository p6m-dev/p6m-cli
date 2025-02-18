use crate::cli::P6mEnvironment;
use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, Duration, Local, Utc};
use jsonwebtokens::raw::{self, TokenSlices};
use log::{debug, trace};
use openid::AccessTokenResponse;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::{self, Display, Formatter},
    fs,
};

use super::openid;

/// Acts as an abstraction for reading and writing tokens from disk.
#[derive(Debug, Clone)]
pub struct TokenRepository {
    pub environment: P6mEnvironment,
    auth_dir: Utf8PathBuf,
    organization_id: Option<String>,
    force: bool,
    scopes: Vec<String>,
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
        debug!("asserting claims: {}", self);
        debug!("desired_claims: {}", desired_claims);

        if desired_claims.login_kubernetes.is_some()
            && self.login_kubernetes != desired_claims.login_kubernetes
        {
            return Err(anyhow::anyhow!("Missing login:kubernetes"));
        }

        if desired_claims.org.is_some() && self.org != desired_claims.org {
            return Err(anyhow::anyhow!("Missing desired org claim"));
        }

        if desired_claims.orgs.is_some() && self.orgs != desired_claims.orgs {
            return Err(anyhow::anyhow!("Missing desired orgs claim"));
        }

        debug!("claims assertion passed");
        Ok(())
    }

    pub fn merge(&mut self, from: Claims) {
        if from.login_kubernetes.is_some() {
            self.login_kubernetes = from.login_kubernetes;
        }
        if from.org.is_some() {
            self.org = from.org;
        }
        if from.orgs.is_some() {
            self.orgs = from.orgs;
        }
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
    pub fn new(environment: &P6mEnvironment) -> Result<TokenRepository> {
        let auth_dir = environment.config_dir().join("auth");
        fs::create_dir_all(&auth_dir)?;

        let mut token_repository = TokenRepository {
            auth_dir,
            organization_id: None,
            environment: environment.clone(),
            force: false,
            scopes: vec![],
            desired_claims: Claims::default(),
        };

        Self::DEFAULT_SCOPES.split(" ").for_each(|scope| {
            token_repository.with_scope(scope, Claims::default());
        });

        Ok(token_repository)
    }

    pub fn force(mut self) -> Self {
        self.force = true;
        self
    }

    pub fn with_organization(&mut self, organization: &String) -> Result<()> {
        let token_repository = Self::new(&self.environment)?;

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

        Ok(())
    }

    pub fn with_scope(&mut self, scope: &str, desired_claims: Claims) {
        self.scopes.push(scope.to_string());
        self.scopes.sort();
        self.scopes.dedup();
        self.desired_claims.merge(desired_claims);
    }

    pub async fn try_login(&mut self) -> Result<Self> {
        let access_token_response = match self.force {
            true => {
                self.clear()?;
                self.login("forced").await?
            }
            false => match self.try_refresh().await?.read_tokens().ok() {
                Some(access_token_response) => access_token_response,
                None => self.login("expired tokens").await?,
            },
        };

        self.assert_claims(&access_token_response, "post login")
            .await?;
        self.write_tokens(&access_token_response)?;

        Ok(self.clone())
    }

    pub async fn try_refresh(&mut self) -> Result<Self> {
        let access_token_response = match (self.force, self.should_refresh()?) {
            (true, _) => self.refresh("forced refresh").await?,
            (_, true) => match self.refresh("expired tokens").await {
                Ok(access_token_response) => access_token_response,
                Err(_) => self.login("expired tokens").await?,
            },
            _ => self.read_tokens()?,
        };

        self.assert_claims(&access_token_response, "post refresh")
            .await?;
        self.write_tokens(&access_token_response)?;

        Ok(self.clone())
    }

    async fn login(&mut self, reason: &str) -> Result<AccessTokenResponse> {
        debug!("attempting login due to: {reason}");
        let device_code_request = openid::DeviceCodeRequest::new(self).await?;

        let access_token_response = device_code_request
            .login()
            .await
            .context("unable to exchange device code for tokens")
            .map_err(|e| {
                debug!("Unable to exchange device code for tokens: {e}");
                e
            })?;

        Ok(access_token_response)
    }

    async fn refresh(&mut self, reason: &str) -> Result<AccessTokenResponse> {
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
        reason: &str,
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
        // Massage scopes through with_scope before returning

        let existing_scopes: Vec<String> = self
            .read_claims(AuthToken::Access)
            .unwrap_or(Some(Claims::default()))
            .unwrap_or_default()
            .scope
            .unwrap_or(Self::DEFAULT_SCOPES.to_string())
            .split(" ")
            .map(|s| s.to_string())
            .collect();

        for scope in existing_scopes {
            self.with_scope(scope.as_str(), Claims::default());
        }

        Ok(self.scopes.join(" "))
    }

    pub async fn form_data(&mut self) -> Result<Vec<(&str, String)>> {
        let mut form: Vec<(&str, String)> = vec![];

        let mut acr_values: Vec<String> = vec![];

        if let Some(organization_id) = self.organization_id.clone() {
            acr_values.push(format!("urn:auth:acr:organization-id:{}", organization_id));
        }

        for scope in self.scope_str().await?.split(" ") {
            acr_values.push(format!("urn:auth:acr:scope:{}", scope));
        }

        if acr_values.len() > 0 {
            form.push(("acr_values".into(), acr_values.join(" ")));
        }

        Ok(form)
    }
}

/// Enumeration of Auth Token Types
#[derive(Debug, strum_macros::Display, Clone)]
pub enum AuthToken {
    #[strum(to_string = "ACCESS_TOKEN")]
    Access,
    #[strum(to_string = "ID_TOKEN")]
    Id,
    #[strum(to_string = "REFRESH_TOKEN")]
    Refresh,
}
