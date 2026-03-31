use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use log::{debug, trace};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use url::Url;
use uuid::Uuid;

use super::openid::{AccessTokenResponse, OpenIdDiscoveryDocument};
use super::TokenRepository;
use crate::auth::TryAuthReason;

/// Generate a PKCE code verifier and its S256 challenge.
fn generate_pkce() -> (String, String) {
    // Use two UUIDs (32 random bytes) as the verifier source
    let bytes: Vec<u8> = Uuid::new_v4()
        .as_bytes()
        .iter()
        .chain(Uuid::new_v4().as_bytes().iter())
        .copied()
        .collect();
    let verifier = URL_SAFE_NO_PAD.encode(&bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// Generate a random state parameter for CSRF protection.
fn generate_state() -> String {
    Uuid::new_v4().to_string()
}

/// Perform interactive browser login using Authorization Code + PKCE.
pub async fn login(
    token_repository: &TokenRepository,
    oidc: &OpenIdDiscoveryDocument,
    reason: &TryAuthReason,
) -> Result<AccessTokenResponse> {
    let auth_n = &token_repository.auth_n;
    let client_id = auth_n.client_id.as_ref().context("missing client_id")?;
    let authorization_endpoint = oidc
        .authorization_endpoint
        .as_ref()
        .context("missing authorization_endpoint in OpenID configuration")?;

    let scopes = auth_n.additional_scopes();
    if scopes.is_empty() {
        return Err(anyhow::anyhow!(
            "No scopes configured on auth provider for interactive login"
        ));
    }

    let (code_verifier, code_challenge) = generate_pkce();
    let state = generate_state();

    // Bind ephemeral localhost port
    let listener =
        TcpListener::bind("127.0.0.1:0").context("unable to bind localhost for OAuth callback")?;
    let port = listener
        .local_addr()
        .context("unable to get local address")?
        .port();
    let redirect_uri = format!("http://localhost:{}/callback", port);

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

    debug!("authorize URL: {}", authorize_url);

    eprintln!("{}, opening browser for authentication...", reason);
    eprintln!();

    if webbrowser::open(authorize_url.as_str()).is_err() {
        eprintln!("Failed to launch browser.");
        eprintln!("Please visit: {}", authorize_url);
    }

    eprintln!("Waiting for authentication...");
    eprintln!();

    // Wait for the callback
    let (code, returned_state) = wait_for_callback(&listener)?;

    // Validate state
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

    debug!("exchanging authorization code at {}", oidc.token_endpoint);

    let raw_response = reqwest::Client::new()
        .post(&oidc.token_endpoint)
        .form(&form)
        .send()
        .await?
        .text()
        .await
        .context("unable to read token response")?;

    trace!("token response: {}", raw_response);

    let response: AccessTokenResponse =
        serde_json::from_str(&raw_response).context("unable to parse token response")?;

    if response.error.is_some() {
        return Err(response.as_error()).context("token exchange failed");
    }

    Ok(response)
}

/// Perform a token refresh for an interactive auth session.
pub async fn refresh(
    token_repository: &TokenRepository,
    oidc: &OpenIdDiscoveryDocument,
    refresh_token: &str,
) -> Result<AccessTokenResponse> {
    let auth_n = &token_repository.auth_n;
    let mut form = auth_n.refresh_form_data(&refresh_token.to_string())?;

    let scopes = auth_n.additional_scopes();
    if !scopes.is_empty() {
        form.insert("scope".into(), scopes);
    }

    debug!("refreshing interactive token at {}", oidc.token_endpoint);

    let raw_response = reqwest::Client::new()
        .post(&oidc.token_endpoint)
        .form(&form)
        .send()
        .await?
        .text()
        .await
        .context("unable to read refresh response")?;

    trace!("refresh response: {}", raw_response);

    let response: AccessTokenResponse =
        serde_json::from_str(&raw_response).context("unable to parse refresh response")?;

    if response.error.is_some() {
        return Err(response.as_error()).context("token refresh failed");
    }

    Ok(response)
}

/// Wait for the OAuth callback on the localhost listener.
/// Returns (authorization_code, state).
fn wait_for_callback(listener: &TcpListener) -> Result<(String, String)> {
    listener
        .set_nonblocking(false)
        .context("unable to set blocking mode")?;

    let (mut stream, _) = listener.accept().context("unable to accept connection")?;

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .context("unable to read request")?;

    debug!("callback request: {}", request_line.trim());

    // Parse the GET request line: "GET /callback?code=...&state=... HTTP/1.1"
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("malformed HTTP request")?;

    let callback_url =
        Url::parse(&format!("http://localhost{}", path)).context("unable to parse callback URL")?;

    // Check for error response from Azure AD
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

        // Send error page
        let body = format!(
            "<html><body><h1>Authentication Failed</h1><p>{}: {}</p></body></html>",
            error, description
        );
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
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

    // Send success page
    let body = "<html><body><h1>Authentication Successful</h1><p>You may close this tab.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());

    Ok((code, returned_state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let (verifier, challenge) = generate_pkce();
        // Verifier should be base64url-encoded 32 bytes = 43 chars
        assert_eq!(verifier.len(), 43);
        // Challenge should be base64url-encoded SHA-256 = 43 chars
        assert_eq!(challenge.len(), 43);
        // Verify the challenge matches the verifier
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, expected);
    }

    #[test]
    fn test_state_generation() {
        let state1 = generate_state();
        let state2 = generate_state();
        // UUID v4 string = 36 chars
        assert_eq!(state1.len(), 36);
        // Should be unique
        assert_ne!(state1, state2);
    }
}
