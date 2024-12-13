use anyhow::{anyhow, Context};
use chrono::{Duration, Utc};
use log::{debug, trace};
use serde::{Deserialize, Serialize};
use std::time;
use tokio::time::sleep;

use crate::{
    auth::{AuthToken, TokenRepository},
    cli::P6mEnvironment,
};

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
    interactive: bool,
    force: bool,
    environment: P6mEnvironment,
    token_repository: TokenRepository,
    openid_configuration: OpenIdDiscoveryDocument,
    organization_id: Option<String>,
}

impl DeviceCodeRequest {
    pub const DEFAULT_SCOPES: &str = "openid email offline_access";

    pub async fn new(environment: &P6mEnvironment) -> Result<Self, anyhow::Error> {
        let token_repository = TokenRepository::new(&environment)?;
        let openid_configuration =
            OpenIdDiscoveryDocument::discover(environment.domain.clone()).await?;

        Ok(Self {
            client_id: environment.client_id.clone(),
            // note: openid, email, offline_access are implicity requested
            scope: DeviceCodeRequest::DEFAULT_SCOPES.into(),
            audience: environment.audience.clone(),
            interactive: true,
            force: false,
            environment: environment.clone(),
            token_repository,
            openid_configuration,
            organization_id: None,
        })
    }

    pub fn with_scope(mut self, scope: &str) -> Self {
        let mut scopes: Vec<&str> = self.scope.split(" ").into_iter().collect::<Vec<_>>();
        scopes.push(scope);
        scopes.sort();
        scopes.dedup();

        debug!("Setting scopes: {:?}", scopes);

        self.scope = scopes.join(" ");
        self
    }

    pub async fn with_organization(mut self, organization: &String) -> Result<Self, anyhow::Error> {
        let token_repository = TokenRepository::new(&self.environment)?;

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

        self.token_repository = token_repository.with_organization_id(&organization_id)?;

        // Copy existing scopes, if any (so not to loose previously granted scopes)
        self.scope = self
            .token_repository
            .read_claims(AuthToken::Access)
            .unwrap_or_default()
            .and_then(|claims| claims.scope)
            .unwrap_or(Self::DEFAULT_SCOPES.to_string());

        self = self.with_scope(&format!("org:{}", organization_id));
        self.organization_id = Some(organization_id.clone());

        Ok(self)
    }

    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    pub fn force_new(mut self) -> Self {
        self.force = true;
        self
    }

    pub async fn exchange_for_token(&self) -> Result<TokenRepository, anyhow::Error> {
        let tokens = match (
            self.token_repository
                .clone()
                .read_token(AuthToken::Refresh)?,
            (self
                .token_repository
                .clone()
                .read_expiration(AuthToken::Access)?
                - Utc::now()
                <= Duration::hours(1)),
            self.force,
        ) {
            (None, _, false) => {
                return Err(anyhow!("Not logged in (force: false)"));
            }
            (None, _, _) | (_, _, true) => {
                if !self.interactive {
                    return Err(anyhow!(
                        "Not logged in (force: {}, interactive: false)",
                        self.force
                    ));
                }
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

                tokens
            }
            (Some(refresh_token), true, _) => self.refresh(&refresh_token).await?,
            _ => self.token_repository.current()?,
        };

        self.token_repository.write_tokens(&tokens)?;
        Ok(self.token_repository.clone())
    }

    async fn refresh(&self, refresh_token: &String) -> Result<AccessTokenResponse, anyhow::Error> {
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

    // Only available if the "https://p6m.dev/v1/org" claim is on the token
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "https://p6m.dev/v1/org")]
    pub org: Option<String>,
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

    pub fn to_string(&self) -> String {
        let detail: Vec<String> = match (self.email.as_ref(), self.org.as_ref()) {
            (Some(email), Some(org)) => {
                vec![
                    format!("Organization: {}", org),
                    format!("Email: {}", email),
                ]
            }
            (Some(email), None) => vec![format!("Email: {}", email)],
            _ => vec![],
        };

        detail.join("\n")
    }

    pub fn to_json(&self) -> Result<String, anyhow::Error> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
