use itertools::Itertools;
use log::{debug, trace};
use serde::{Deserialize, Serialize};
use std::time;
use tokio::time::sleep;

use crate::cli::P6mEnvironment;

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
    pub client_id: String,
    pub scope: String,
    pub audience: String,
    openid_configuration: OpenIdDiscoveryDocument,
    organization_id: Option<String>,
    desired_claims: Vec<String>,
}

impl DeviceCodeRequest {
    pub const DEFAULT_SCOPES: &str = "openid email offline_access login:cli";

    pub async fn new(environment: &P6mEnvironment) -> Result<Self, anyhow::Error> {
        let openid_configuration =
            OpenIdDiscoveryDocument::discover(environment.domain.clone()).await?;

        Ok(Self {
            client_id: environment.client_id.clone(),
            // note: openid, email, offline_access are implicity requested
            scope: DeviceCodeRequest::DEFAULT_SCOPES.into(),
            audience: environment.audience.clone(),
            openid_configuration,
            organization_id: None,
            desired_claims: vec![],
        })
    }

    pub fn with_scope(&mut self, scope: &str) -> Self {
        let mut scopes: Vec<&str> = self.scope.split(" ").into_iter().collect::<Vec<_>>();
        scopes.push(scope);
        scopes.sort();
        scopes.dedup();

        if scope.starts_with("login:") {
            let claim = scope.split(":").join("/");
            let desired_claim = format!("https://p6m.dev/v1/permission/{}", claim);
            debug!("Adding desired claim: {}", desired_claim);
            self.desired_claims.push(desired_claim);
        }

        debug!("Setting scopes: {:?}", scopes);
        self.scope = scopes.join(" ");
        self.clone()
    }

    pub async fn login(&self) -> Result<AccessTokenResponse, anyhow::Error> {
        let device_code_response = self
            .send(
                self.openid_configuration
                    .device_authorization_endpoint
                    .clone(),
            )
            .await?;

        let tokens = device_code_response
            .exchange_for_token(
                self.openid_configuration.token_endpoint.clone(),
                self.client_id.clone(),
            )
            .await?;

        Ok(tokens)
    }

    pub async fn refresh(
        &self,
        refresh_token: &String,
    ) -> Result<AccessTokenResponse, anyhow::Error> {
        let mut form: Vec<(&str, String)> = vec![
            ("grant_type", "refresh_token".into()),
            ("client_id", self.client_id.to_string()),
            ("refresh_token", refresh_token.to_string()),
        ];

        if let Some(organization_id) = self.organization_id.clone() {
            form.push((
                "acr_values".into(),
                format!("urn:auth:acr:organization-id:{}", organization_id),
            ));
        }

        let raw_response = reqwest::Client::new()
            .post(self.openid_configuration.token_endpoint.clone())
            .form(&form)
            .send()
            .await?
            .text()
            .await?;

        trace!("Refresh token response: {}", raw_response);
        let response: AccessTokenResponse = serde_json::from_str(raw_response.as_str())?;

        if response.error.is_some() {
            return Err(response.as_error());
        }

        Ok(response)
    }

    async fn send(
        &self,
        device_authorization_endpoint: String,
    ) -> Result<DeviceCodeResponse, anyhow::Error> {
        debug!(
            "Sending device code request to {} with scopes {}",
            device_authorization_endpoint, self.scope,
        );
        let client = reqwest::Client::new();
        let raw_response = client
            .post(device_authorization_endpoint)
            .form(&[
                ("client_id", self.client_id.clone()),
                ("scope", self.scope.clone()),
                ("audience", self.audience.clone()),
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
