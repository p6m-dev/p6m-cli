use anyhow::Context;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use log::{debug, trace};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sha2::{Digest, Sha256};
use std::{
    io::{stderr, stdin, BufRead, BufReader, Write},
    net::TcpListener,
    time,
};
use tokio::time::sleep;
use url::Url;
use uuid::Uuid;

use crate::{auth::serde::deserialize_string_option, AuthN};

use super::{TokenRepository, TryAuthReason};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenIdDiscoveryDocument {
    pub issuer: String,
    pub token_endpoint: String,
    pub device_authorization_endpoint: Option<String>,
    pub authorization_endpoint: Option<String>,
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

    pub async fn login(
        &self,
        reason: &TryAuthReason,
    ) -> Result<AccessTokenResponse, anyhow::Error> {
        if self.token_repository.auth_n.is_interactive() {
            return self.login_pkce(reason).await;
        }

        let device_code_response = self.send().await.map_err(|e| {
            debug!("Failed to send device code request: {}", e);
            e
        })?;

        let tokens = device_code_response
            .exchange_for_token(
                &self.openid_configuration,
                &self.token_repository.auth_n,
                reason,
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
        let mut form = self
            .token_repository
            .auth_n
            .refresh_form_data(refresh_token)?;

        if self.token_repository.auth_n.is_interactive() {
            form.insert(
                "scope".into(),
                self.token_repository.auth_n.additional_scopes().join(" "),
            );
        } else {
            form.extend(self.token_repository.acr_values_form_data().await?);
        }

        let raw_response = reqwest::Client::new()
            // codeql[rust/request-forgery] token_endpoint from trusted OIDC discovery, not user input
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

    /// Interactive browser login using Authorization Code + PKCE.
    /// Used when the auth provider has a localhost redirect_uri configured.
    async fn login_pkce(
        &self,
        reason: &TryAuthReason,
    ) -> Result<AccessTokenResponse, anyhow::Error> {
        let auth_n = &self.token_repository.auth_n;
        let client_id = auth_n.client_id.as_ref().context("missing client_id")?;
        let authorization_endpoint = self
            .openid_configuration
            .authorization_endpoint
            .as_ref()
            .context("missing authorization_endpoint in OpenID configuration")?;

        let scopes = auth_n.additional_scopes().join(" ");
        if scopes.is_empty() {
            return Err(anyhow::anyhow!(
                "No scopes configured on auth provider for interactive login"
            ));
        }

        // Parse redirect_uri from params to determine bind address
        let configured_redirect = auth_n
            .redirect_uri()
            .context("missing redirect_uri in auth provider params")?;
        let mut parsed_redirect =
            Url::parse(configured_redirect).context("unable to parse redirect_uri")?;
        let host = parsed_redirect.host_str().unwrap_or("localhost");
        let bind_addr = format!("{}:0", host);

        // PKCE
        let bytes: Vec<u8> = Uuid::new_v4()
            .as_bytes()
            .iter()
            .chain(Uuid::new_v4().as_bytes().iter())
            .copied()
            .collect();
        let code_verifier = URL_SAFE_NO_PAD.encode(&bytes);
        let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let state = Uuid::new_v4().to_string();

        // Bind ephemeral port on the configured host
        let listener = TcpListener::bind(&bind_addr)
            .context(format!("unable to bind {} for OAuth callback", bind_addr))?;
        let port = listener
            .local_addr()
            .context("unable to get local address")?
            .port();
        parsed_redirect
            .set_port(Some(port))
            .map_err(|_| anyhow::anyhow!("unable to set port on redirect_uri"))?;
        let redirect_uri = parsed_redirect
            .to_string()
            .trim_end_matches('/')
            .to_string();

        // Build authorize URL
        let mut authorize_url =
            Url::parse(authorization_endpoint).context("unable to parse authorization_endpoint")?;
        authorize_url
            .query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("scope", &scopes)
            .append_pair("code_challenge", &code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);

        trace!(
            "login_pkce redirect_uri: {}, state: {}",
            redirect_uri,
            state
        );

        eprintln!("{}, opening browser for authentication...", reason);
        eprintln!();

        if webbrowser::open(authorize_url.as_str()).is_err() {
            eprintln!("Failed to launch browser.");
            eprintln!("Please visit: {}", authorize_url);
        }

        eprintln!("Waiting for authentication...");
        eprintln!();

        // Wait for callback with 120s timeout
        trace!("login_pkce waiting for callback on port {}", port);
        let (code, returned_state) = tokio::time::timeout(
            time::Duration::from_secs(120),
            tokio::task::spawn_blocking(move || Self::wait_for_callback(&listener)),
        )
        .await
        .context("authentication timed out after 120 seconds")?
        .context("callback handler failed")?
        .context("unable to process callback")?;
        trace!("login_pkce got callback, code len={}", code.len());

        if returned_state != state {
            return Err(anyhow::anyhow!(
                "OAuth state mismatch — possible CSRF attack"
            ));
        }

        // Exchange code for tokens
        let form = [
            ("client_id", client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect_uri),
            ("code_verifier", &code_verifier),
            ("scope", &scopes),
        ];

        debug!(
            "login_pkce exchanging code at {}",
            self.openid_configuration.token_endpoint
        );

        let raw_response = reqwest::Client::new()
            // codeql[rust/request-forgery] token_endpoint from trusted OIDC discovery, not user input
            .post(&self.openid_configuration.token_endpoint)
            .form(&form)
            .send()
            .await?
            .text()
            .await
            .context("unable to read token response")?;

        trace!("login_pkce token response: {}", raw_response);

        let response: AccessTokenResponse =
            serde_json::from_str(&raw_response).context("unable to parse token response")?;

        if response.error.is_some() {
            return Err(response.as_error()).context("token exchange failed");
        }

        debug!(
            "login_pkce success: access_token={}, id_token={}, refresh_token={}",
            response.access_token.is_some(),
            response.id_token.is_some(),
            response.refresh_token.is_some()
        );

        Ok(response)
    }

    /// Wait for the OAuth callback on the localhost listener.
    fn wait_for_callback(listener: &TcpListener) -> Result<(String, String), anyhow::Error> {
        listener
            .set_nonblocking(false)
            .context("unable to set blocking mode")?;

        let (mut stream, addr) = listener.accept().context("unable to accept connection")?;
        trace!("callback accepted connection from {}", addr);

        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .context("unable to read request")?;

        trace!("callback request: {}", request_line.trim());

        let path = request_line
            .split_whitespace()
            .nth(1)
            .context("malformed HTTP request")?;

        let callback_url = Url::parse(&format!("http://localhost{}", path))
            .context("unable to parse callback URL")?;

        // Check for error response
        if let Some(error) = callback_url
            .query_pairs()
            .find(|(k, _)| k == "error")
            .map(|(_, v)| v.to_string())
        {
            let description = callback_url
                .query_pairs()
                .find(|(k, _)| k == "error_description")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();

            let body = format!(
                "<html><body><h1>Authentication Failed</h1><p>{}: {}</p></body></html>",
                error, description
            );
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            let _ = stream.write_all(response.as_bytes());

            return Err(anyhow::anyhow!("{}: {}", error, description));
        }

        let code = callback_url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| v.to_string())
            .context("missing authorization code in callback")?;

        let returned_state = callback_url
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.to_string())
            .context("missing state in callback")?;

        let body = "<html><body><h1>Authentication Successful</h1><p>You may close this tab.</p></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        let _ = stream.write_all(response.as_bytes());

        Ok((code, returned_state))
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
        reason: &TryAuthReason,
    ) -> Result<AccessTokenResponse, anyhow::Error> {
        let url = self
            .verification_uri_complete
            .as_ref()
            .or(self.verification_uri.as_ref())
            .or(self.verification_url.as_ref())
            .context("missing verification URL")?;

        let host = Url::parse(url)?
            .clone()
            .host()
            .context("missing host")?
            .to_string();

        eprintln!("{}, authentication with {} is necessary.", reason, host);
        eprintln!();
        eprintln!("First copy your one-time code: {}", self.user_code);
        eprintln!();
        eprintln!("Press Enter to open {} in your browser...", host);
        stderr().flush()?;
        stdin().read_line(&mut String::new())?;

        if webbrowser::open(url).is_err() {
            eprintln!("Failed to launch browser");
            eprintln!("Please visit {} and enter the code.", url)
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

    pub fn as_error(&self) -> anyhow::Error {
        anyhow::anyhow!(
            "{}: {}",
            self.error.clone().unwrap_or_default(),
            self.error_description.clone().unwrap_or_default()
        )
    }
}
