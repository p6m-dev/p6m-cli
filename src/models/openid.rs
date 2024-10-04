use log::{debug, trace};
use serde::{Deserialize, Serialize};
use std::time;
use tokio::time::sleep;

#[derive(Clone, Deserialize, Serialize)]
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

pub struct DeviceCodeRequest {
    pub client_id: String,
    pub scope: String,
    pub audience: String,
}

impl DeviceCodeRequest {
    pub async fn send(
        &self,
        device_authorization_endpoint: String,
    ) -> Result<DeviceCodeResponse, anyhow::Error> {
        debug!(
            "Sending device code request to {}",
            device_authorization_endpoint
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
    pub async fn exchange_for_token(
        &self,
        token_endpoint: String,
        client_id: String,
    ) -> Result<AccessTokenResponse, anyhow::Error> {
        match webbrowser::open(&self.verification_uri_complete) {
            Ok(_) => {
                println!(
                    "Verify that the browser shows the following code and approve the request."
                );
            }
            Err(_) => {
                println!("Failed to launch browser");
                println!(
                    "Please visit {} and enter the code manually.",
                    self.verification_uri
                )
            }
        }

        println!();
        println!("{}", self.user_code);
        println!();
        println!(
            "The code will expire in {} minutes.",
            chrono::Duration::seconds(self.expires_in as i64).num_minutes()
        );
        println!("Waiting for approval...");

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

#[derive(Deserialize, Serialize, Clone)]
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
    pub fn has_token(&self) -> bool {
        self.access_token.is_some()
    }

    #[allow(dead_code)]
    pub fn is_pending(&self) -> bool {
        self.error
            .clone()
            .is_some_and(|e| e == "authorization_pending")
    }

    pub fn is_expired(&self) -> bool {
        self.error.clone().is_some_and(|e| e == "expired_token")
    }

    pub fn is_denied(&self) -> bool {
        self.error.clone().is_some_and(|e| e == "access_denied")
    }

    pub fn as_error(&self) -> anyhow::Error {
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
}

impl UserInfo {
    pub async fn request(
        userinfo_endpoint: String,
        access_token: String,
    ) -> Result<Self, anyhow::Error> {
        debug!("Requesting user info from {}", userinfo_endpoint);
        let raw_response = reqwest::Client::new()
            .get(userinfo_endpoint)
            .bearer_auth(access_token)
            .header("Content-Type", "application/json")
            .send()
            .await?
            .text()
            .await?;
        debug!("User info response: {}", raw_response);
        Ok(serde_json::from_str(&raw_response)?)
    }
}
