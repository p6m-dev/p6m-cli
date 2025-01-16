use crate::cli::P6mEnvironment;
use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, Duration, Utc};
use jsonwebtokens::raw::{self, TokenSlices};
use log::debug;
use openid::AccessTokenResponse;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs};

use super::openid;

/// Acts as an abstraction for reading and writing tokens from disk.
#[derive(Debug, Clone)]
pub struct TokenRepository {
    auth_dir: Utf8PathBuf,
    organization_id: Option<String>,
    environment: P6mEnvironment,
    force: bool,
    scopes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Claims {
    pub exp: Option<i64>,
    #[serde(rename = "https://p6m.dev/v1/orgs")]
    pub orgs: Option<BTreeMap<String, String>>,
    pub scope: Option<String>,
    pub email: Option<String>,
    #[serde(rename = "https://p6m.dev/v1/org")]
    pub org: Option<String>,
}

impl TokenRepository {
    /// Creates a [TokenRepository] given a [P6mEnvironment].
    pub fn new(environment: &P6mEnvironment) -> Result<TokenRepository> {
        let auth_dir = environment.config_dir().join("auth");
        fs::create_dir_all(&auth_dir)?;
        Ok(TokenRepository {
            auth_dir,
            organization_id: None,
            environment: environment.clone(),
            force: false,
            scopes: vec![],
        })
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
            .orgs
            .and_then(|orgs| {
                orgs.into_iter()
                    .find(|org| {
                        // match on either the key (org id) or the value (org name)
                        org.0 == organization.to_string() || org.1 == organization.to_string()
                    })
                    .map(|o| o.0)
            })
            .context("missing desired organization in access token claims")?;

        self.with_organization_id(&organization_id)?;

        Ok(())
    }

    pub fn with_scope(&mut self, scope: &str) {
        self.scopes.push(scope.to_string());
    }

    pub async fn try_login(&mut self) -> Result<()> {
        if self.force {
            self.clear()?;
            self.login().await?;
        } else if self.should_refresh()? {
            match self.refresh().await {
                Ok(_) => return Ok(()),
                Err(_) => self.login().await?,
            };
        }

        let granted_scopes: Vec<String> = self
            .read_claims(AuthToken::Access)?
            .context("unable to read claims")?
            .scope
            .context("missing scope claim")?
            .split(" ")
            .map(|s| s.to_string())
            .collect();

        if !granted_scopes.iter().any(|s| !self.scopes.contains(s)) {
            debug!(
                "Desired scopes missing, re-authenticating. (granted: {}, desired: {})",
                granted_scopes.join(" "),
                self.scopes.join(" "),
            );
            self.login().await?;
        }

        Ok(())
    }

    async fn login(&mut self) -> Result<Self> {
        let mut device_code_request = openid::DeviceCodeRequest::new(&self.environment).await?;

        for scope in self.scopes.iter() {
            device_code_request = device_code_request.with_scope(scope);
        }

        let access_token_response = device_code_request
            .login()
            .await
            .context("unable to exchange device code for tokens")?;

        self.write_tokens(&access_token_response)?;

        Ok(self.clone())
    }

    async fn refresh(&mut self) -> Result<Self> {
        let refresh_token = self
            .read_token(AuthToken::Refresh)
            .context("unable to read refresh token")?
            .context("missing refresh token")?;

        let device_code_request = openid::DeviceCodeRequest::new(&self.environment).await?;

        let access_token_response = device_code_request
            .refresh(&refresh_token)
            .await
            .context("unable to refresh tokens")?;

        self.write_tokens(&access_token_response)?;

        Ok(self.clone())
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
        self.scopes.push(format!("org:{}", organization_id));
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
            debug!("Reading {path}");
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

    #[allow(dead_code)]
    pub fn claim_keys(&self, token_type: AuthToken) -> Result<Vec<String>> {
        let claims = match self
            .read_token(token_type)
            .context("missing token")?
            .clone()
        {
            Some(token) => {
                let TokenSlices { claims, .. } =
                    raw::split_token(&token).context("unable to split token")?;

                raw::decode_json_token_slice(claims)?
                    .as_object()
                    .context("unable to convert claims to object")?
                    .keys()
                    .map(|k| k.to_string())
                    .collect()
            }
            None => vec![],
        };

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
        let id_exp = self.clone().read_expiration(AuthToken::Id)?;
        let access_exp = self.clone().read_expiration(AuthToken::Access)?;

        let access_token_will_exp = access_exp < Utc::now() - Duration::hours(1);
        let id_token_will_exp = id_exp < Utc::now() - Duration::hours(1);

        debug!("Access token expiring? {access_token_will_exp}");
        debug!("Id token expiring? {id_token_will_exp}");

        Ok(access_token_will_exp || id_token_will_exp)
    }

    // Get the expiration date of the desired token
    pub fn read_expiration(self, token_type: AuthToken) -> Result<DateTime<Utc>> {
        let claims = self.read_claims(token_type.clone())?.unwrap_or_default();

        let exp = DateTime::from_timestamp(claims.exp.unwrap_or(Utc::now().timestamp()), 0)
            .context("unable to parse exp claim")?;

        debug!("{token_type} expiration: {exp}");

        Ok(exp)
    }

    #[allow(dead_code)]
    pub fn has_claims(&self, token_type: AuthToken, desired_claims: &Vec<String>) -> Result<bool> {
        let actual_claims = self.claim_keys(token_type)?;

        debug!("Actual claims: {:?}", actual_claims);

        Ok(desired_claims
            .into_iter()
            .all(|c| actual_claims.contains(c)))
    }

    /// Write a token to disk.
    pub fn write_token(&self, token_type: AuthToken, token: Option<&String>) -> Result<()> {
        if let Some(token) = token {
            let path = self.token_path(token_type);
            debug!("Writing {path}");
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
            let detail: Vec<String> = match (claims.email.as_ref(), claims.org.as_ref()) {
                (Some(email), Some(org)) => {
                    vec![
                        format!("Organization: {}", org),
                        format!("Email: {}", email),
                    ]
                }
                (Some(email), None) => vec![format!("Email: {}", email)],
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
