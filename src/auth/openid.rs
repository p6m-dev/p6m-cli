use log::{debug, trace};
use serde::{Deserialize, Serialize};
use std::time;
use tokio::time::sleep;

use super::TokenRepository;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenIdDiscoveryDocument {
    pub issuer: String,
    pub token_endpoint: String,
    pub device_authorization_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
}

impl OpenIdDiscoveryDocument {
    pub async fn discover(domain: String) -> Result<Self, anyhow::Error> {
        let url = format!("https://{}/.well-known/openid-configuration", domain);
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
            OpenIdDiscoveryDocument::discover(token_repository.environment.domain.clone()).await?;

        Ok(Self {
            token_repository: token_repository.clone(),
            openid_configuration,
        })
    }

    pub async fn login(&self) -> Result<AccessTokenResponse, anyhow::Error> {
        let device_code_response = self
            .send(
                self.openid_configuration
                    .device_authorization_endpoint
                    .clone(),
            )
            .await
            .map_err(|e| {
                debug!("Failed to send device code request: {}", e);
                e
            })?;

        let tokens = device_code_response
            .exchange_for_token(
                self.openid_configuration.token_endpoint.clone(),
                self.token_repository.environment.client_id.clone(),
            )
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
        let mut form: Vec<(&str, String)> = vec![
            ("grant_type", "refresh_token".into()),
            (
                "client_id",
                self.token_repository.environment.client_id.to_string(),
            ),
            ("refresh_token", refresh_token.to_string()),
        ];

        form.extend(self.token_repository.form_data().await?);

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

    async fn send(
        &self,
        device_authorization_endpoint: String,
    ) -> Result<DeviceCodeResponse, anyhow::Error> {
        let scope = self.token_repository.clone().scope_str().await?;
        debug!(
            "Sending device code request to {} with scopes {}",
            device_authorization_endpoint, scope,
        );
        let client = reqwest::Client::new();
        let raw_response = client
            .post(device_authorization_endpoint)
            .form(&[
                (
                    "client_id",
                    self.token_repository.environment.client_id.clone(),
                ),
                ("scope", scope),
                (
                    "audience",
                    self.token_repository.environment.audience.clone(),
                ),
            ])
            .send()
            .await?
            .text()
            .await?;
        trace!("Device code response: {}", raw_response);
        Ok(serde_json::from_str(&raw_response)?)
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u32,
    pub interval: u32,
}

impl DeviceCodeResponse {
    async fn exchange_for_token(
        &self,
        token_endpoint: String,
        client_id: String,
    ) -> Result<AccessTokenResponse, anyhow::Error> {
        match webbrowser::open(&self.verification_uri_complete) {
            Ok(_) => {
                eprintln!(
                    "Verify that the browser shows the following code and approve the request."
                );
            }
            Err(_) => {
                eprintln!("Failed to launch browser");
                eprintln!(
                    "Please visit {} and enter the code manually.",
                    self.verification_uri
                )
            }
        }

        eprintln!();
        eprintln!("{}", self.user_code);
        eprintln!();
        eprintln!(
            "The code will expire in {} minutes.",
            chrono::Duration::seconds(self.expires_in as i64).num_minutes()
        );
        eprintln!("Waiting for approval...");

        loop {
            // Wait the specified amount of time before polling for an access token
            sleep(time::Duration::from_secs(self.interval as u64)).await;

            let client = reqwest::Client::new();
            let raw_response = client
                .post(token_endpoint.clone())
                .form(&[
                    ("client_id", client_id.clone()),
                    ("device_code", self.device_code.clone()),
                    (
                        "grant_type",
                        "urn:ietf:params:oauth:grant-type:device_code".to_owned(),
                    ),
                ])
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
                self.interval
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
