use crate::auth::TokenRepository;
use crate::{
    cli::P6mEnvironment,
    models::openid::{AccessTokenResponse, DeviceCodeRequest, OpenIdDiscoveryDocument},
    whoami,
};
use anyhow::Error;
use clap::ArgMatches;
use log::{debug, trace};

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    let token_repository = TokenRepository::new(&environment)?;

    let required_scopes = vec![
        "openid",
        "email",
        "offline_access",
        "roles",
        "login:archetect",
        "login:kubernetes",
    ];

    let openid_configuration =
        OpenIdDiscoveryDocument::discover(environment.domain.clone()).await?;
    let device_code_response = DeviceCodeRequest {
        client_id: environment.client_id.clone(),
        scope: required_scopes.join(" "),
        audience: environment.audience.clone(),
    }
    .send(openid_configuration.device_authorization_endpoint)
    .await?;
    let tokens = device_code_response
        .exchange_for_token(
            openid_configuration.token_endpoint,
            environment.client_id.clone(),
        )
        .await?;

    token_repository.write_tokens(&tokens)?;

    whoami::execute(environment, matches).await
}

pub async fn update_token(
    openid_configuration: &OpenIdDiscoveryDocument,
    environment: &P6mEnvironment,
    refresh_token: Option<String>,
) -> Result<AccessTokenResponse, Error> {
    debug!("Access token expired, attempting to refresh.");
    let token_manager = TokenRepository::new(&environment)?;

    if let Some(token) = refresh_token {
        let raw_response = reqwest::Client::new()
            .post(openid_configuration.token_endpoint.clone())
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", environment.client_id.as_str()),
                ("refresh_token", token.as_str()),
            ])
            .send()
            .await?
            .text()
            .await?;
        trace!("Refresh token response: {}", raw_response);
        let response: AccessTokenResponse = serde_json::from_str(raw_response.as_str())?;

        if response.error.is_some() {
            return Err(response.as_error());
        }

        token_manager.write_tokens(&response)?;

        Ok(response)
    } else {
        Err(anyhow::anyhow!(
            "Access token expired and no refresh token found."
        ))
    }
}
