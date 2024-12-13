use crate::models::openid::{AccessTokenResponse, OpenIdDiscoveryDocument};
use crate::{cli::P6mEnvironment, models::openid::UserInfo};
use anyhow::{anyhow, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, NaiveDateTime, Utc};
use jsonwebtokens::raw::{self, TokenSlices};
use log::debug;
use serde::Deserialize;
use std::{collections::BTreeMap, fs};

/// Acts as an abstraction for reading and writing tokens from disk.
#[derive(Debug, Clone)]
pub struct TokenRepository {
    auth_dir: Utf8PathBuf,
    environment: P6mEnvironment,
}

#[derive(Debug, Deserialize, Default)]
pub struct Claims {
    pub exp: i64,
    #[serde(rename = "https://p6m.dev/v1/orgs")]
    pub orgs: Option<BTreeMap<String, String>>,
    pub scope: Option<String>,
}

impl TokenRepository {
    /// Creates a [TokenRepository] given a [P6mEnvironment].
    pub fn new(environment: &P6mEnvironment) -> Result<TokenRepository> {
        let auth_dir = environment.config_dir().join("auth");
        fs::create_dir_all(&auth_dir)?;
        Ok(TokenRepository {
            auth_dir,
            environment: environment.clone(),
        })
    }

    /// Appends organization_id to the path for the stored tokens
    pub fn with_organization_id(mut self, organization_id: &String) -> Result<Self> {
        self.auth_dir = self.auth_dir.join(organization_id);
        fs::create_dir_all(&self.auth_dir)?;
        Ok(self)
    }

    /// Get the User Info from OpenID
    pub async fn user_info(&self) -> Result<UserInfo> {
        let openid_configuration =
            OpenIdDiscoveryDocument::discover(self.environment.domain.clone()).await?;

        let token = self
            .read_token(AuthToken::Access)?
            .context("missing access token")?;

        match UserInfo::request(openid_configuration.userinfo_endpoint, token)
            .await
            .context("unable to get user info")
        {
            Ok(user_info) => Ok(user_info),
            Err(_) => Err(anyhow!("unable to get user info")),
        }
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

    // Get the expiration date of the desired token
    pub fn read_expiration(self, token_type: AuthToken) -> Result<DateTime<Utc>> {
        let claims = self.read_claims(token_type)?.unwrap_or_default();

        Ok(NaiveDateTime::from_timestamp_opt(claims.exp, 0)
            .map(|dt| dt.and_utc())
            .context("unable to parse exp claim")?)
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

    pub fn current(&self) -> Result<AccessTokenResponse> {
        Ok(AccessTokenResponse {
            access_token: self.read_token(AuthToken::Access)?,
            refresh_token: self.read_token(AuthToken::Refresh)?,
            id_token: self.read_token(AuthToken::Id)?,
            error: None,
            error_description: None,
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
}

/// Enumeration of Auth Token Types
#[derive(Debug, strum_macros::Display)]
pub enum AuthToken {
    #[strum(to_string = "ACCESS_TOKEN")]
    Access,
    #[strum(to_string = "ID_TOKEN")]
    Id,
    #[strum(to_string = "REFRESH_TOKEN")]
    Refresh,
}
