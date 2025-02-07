use anyhow::Context;
use log::{debug, trace};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::{
    io::{stderr, stdin, Write},
    time,
};
use tokio::time::sleep;
use url::Url;

use crate::{auth::serde::deserialize_string_option, AuthN};

use super::TokenRepository;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenIdDiscoveryDocument {
    pub issuer: String,
    pub token_endpoint: String,
    pub device_authorization_endpoint: Option<String>,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
}

impl OpenIdDiscoveryDocument {
    pub async fn discover(auth_n: &AuthN) -> Result<Self, anyhow::Error> {
        let url = auth_n
            .discovery_uri
            .clone()
            .context("missing discovery uri")?;
        debug!("Fetching OpenID configuration from {}", url);
        let raw_response = reqwest::get(&url).await?.text().await?;
        trace!("OpenID configuration response: {}", raw_response);
        Ok(serde_json::from_str(&raw_response)?)
    }
}

#[derive(Debug, Clone)]
pub struct DeviceCodeRequest {
    token_repository: TokenRepository,
    openid_configuration: OpenIdDiscoveryDocument,
}

impl DeviceCodeRequest {
    pub async fn new(token_repository: &TokenRepository) -> Result<Self, anyhow::Error> {
        let openid_configuration =
            OpenIdDiscoveryDocument::discover(&token_repository.auth_n).await?;

        Ok(Self {
            token_repository: token_repository.clone(),
            openid_configuration,
        })
    }

    pub async fn login(&self) -> Result<AccessTokenResponse, anyhow::Error> {
        let device_code_response = self.send().await.map_err(|e| {
            debug!("Failed to send device code request: {}", e);
            e
        })?;

        let tokens = device_code_response
            .exchange_for_token(&self.openid_configuration, &self.token_repository.auth_n)
            .await
            .map_err(|e| {
                debug!("Failed to exchange device code for token: {}", e);
                e
            })?;

        Ok(tokens)
    }

    pub async fn refresh(
        &mut self,
        refresh_token: &String,
    ) -> Result<AccessTokenResponse, anyhow::Error> {
        let mut form = self
            .token_repository
            .auth_n
            .refresh_form_data(refresh_token)?;

        form.extend(self.token_repository.acr_values_form_data().await?);

        let raw_response = reqwest::Client::new()
            .post(self.openid_configuration.token_endpoint.clone())
            .form(&form)
            .send()
            .await?
            .text()
            .await
            .map_err(|e| {
                debug!("Failed to send refresh token request: {}", e);
                e
            })?;

        trace!("Refresh token response: {}", raw_response);
        let response: AccessTokenResponse =
            serde_json::from_str(raw_response.as_str()).map_err(|e| {
                debug!("Failed to parse refresh token response: {}", e);
                e
            })?;

        if response.error.is_some() {
            return Err(response.as_error()).map_err(|e| {
                debug!("Failed to refresh token: {}", e);
                e
            });
        }

        Ok(response)
    }

    async fn send(&self) -> Result<DeviceCodeResponse, anyhow::Error> {
        let url = self
            .openid_configuration
            .clone()
            .device_authorization_endpoint
            .context("missing device authorization endpoint")?;

        let login_form_data = self
            .token_repository
            .auth_n
            .login_form_data(&self.token_repository.clone().scope_str().await?)?;

        debug!(
            "Sending device code request to {} with data {:?}",
            url, login_form_data,
        );

        let client = reqwest::Client::new();
        let raw_response = client
            .post(&url)
            .form(&login_form_data)
            .send()
            .await?
            .text()
            .await?;

        trace!("Device code response: {}", raw_response);

        Ok(serde_json::from_str(&raw_response)?)
    }
}

#[serde_as]
#[derive(Deserialize, Serialize, Clone)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_url: Option<String>,
    pub verification_uri: Option<String>,
    pub verification_uri_complete: Option<String>,

    #[serde(deserialize_with = "deserialize_string_option")]
    pub expires_in: Option<String>,
    #[serde(deserialize_with = "deserialize_string_option")]
    pub interval: Option<String>,
}

impl DeviceCodeResponse {
    async fn exchange_for_token(
        &self,
        oidc: &OpenIdDiscoveryDocument,
        auth_n: &AuthN,
    ) -> Result<AccessTokenResponse, anyhow::Error> {
        let url = self
            .verification_uri_complete
            .as_ref()
            .or(self.verification_uri.as_ref())
            .or(self.verification_url.as_ref())
            .context("missing verification URL")?;

        eprintln!();
        eprintln!(
            "First copy your one-time code: {}",
            format!("{}", self.user_code)
        );
        eprintln!();
        eprintln!(
            "Press Enter to open {} in your browser...",
            Url::parse(url)?.host().context("missing host")?
        );
        stderr().flush()?;
        stdin().read_line(&mut String::new())?;

        match webbrowser::open(url) {
            Err(_) => {
                eprintln!("Failed to launch browser");
                eprintln!("Please visit {} and enter the code.", url)
            }
            _ => {}
        }

        eprintln!();
        eprintln!("Waiting for approval...");
        eprintln!();

        loop {
            // Wait the specified amount of time before polling for an access token
            sleep(time::Duration::from_secs(
                self.interval.clone().unwrap_or_default().parse::<u64>()?,
            ))
            .await;

            let client = reqwest::Client::new();
            let raw_response = client
                .post(oidc.token_endpoint.clone())
                .form(&auth_n.device_code_form_data(&self.device_code)?)
                .send()
                .await?
                .text()
                .await?;

            trace!("Access token response: {}", raw_response);

            let response: AccessTokenResponse = serde_json::from_str(&raw_response)?;
            if response.has_token() {
                return Ok(response);
            } else if response.is_expired() {
                return Err(anyhow::Error::msg("Device code expired."));
            } else if response.is_denied() {
                return Err(anyhow::Error::msg("User denied request."));
            }

            debug!(
                "Access token not yet available. Will try again in {} seconds.",
                self.interval.clone().unwrap_or_default()
            );
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Default)]
pub struct AccessTokenResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

impl AccessTokenResponse {
    fn has_token(&self) -> bool {
        self.access_token.is_some()
    }

    #[allow(dead_code)]
    fn is_pending(&self) -> bool {
        self.error
            .clone()
            .is_some_and(|e| e == "authorization_pending")
    }

    fn is_expired(&self) -> bool {
        self.error.clone().is_some_and(|e| e == "expired_token")
    }

    fn is_denied(&self) -> bool {
        self.error.clone().is_some_and(|e| e == "access_denied")
    }

    fn as_error(&self) -> anyhow::Error {
        anyhow::anyhow!(
            "{}: {}",
            self.error.clone().unwrap_or_default(),
            self.error_description.clone().unwrap_or_default()
        )
    }
}

/// The UserInfo response. Some fields may be missing depending on the scopes requested.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct UserInfo {
    // Only available if the "profile" scope was requested on the token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    // Only available if the "email" scope was requested on the token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "https://p6m.dev/v1/email")]
    pub p6m_email: Option<String>,

    // Only available if the "https://p6m.dev/v1/org" claim is on the token
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "https://p6m.dev/v1/org")]
    pub org: Option<String>,
}
