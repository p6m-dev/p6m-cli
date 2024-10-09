use crate::cli::P6mEnvironment;
use crate::models::openid::AccessTokenResponse;
use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use log::debug;
use std::fs;

/// Acts as an abstraction for reading and writing tokens from disk.
pub struct TokenRepository {
    auth_dir: Utf8PathBuf,
}

impl TokenRepository {
    /// Creates a [TokenRepository] given a [P6mEnvironment].
    pub fn new(environment: &P6mEnvironment) -> Result<TokenRepository> {
        let auth_dir = environment.config_dir().join("auth");
        fs::create_dir_all(&auth_dir)?;
        Ok(TokenRepository { auth_dir })
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
