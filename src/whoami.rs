use crate::auth::{AuthToken, TokenRepository};
use crate::cli::P6mEnvironment;
use crate::models::openid::DeviceCodeRequest;
use anyhow::{Context, Error};
use clap::ArgMatches;
use log::debug;

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum Output {
    Default,
    Json,
    K8sAuth,
}

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    let output = matches
        .try_get_one("output")
        .unwrap_or(Some(&Output::Default));

    let mut device_code_request = DeviceCodeRequest::new(&environment)
        .await?
        .interactive(false);

    let organization = matches.get_one::<String>("organization-name");

    let mut token_repository = device_code_request
        .exchange_for_token()
        .await
        .context("You are not logged in. First run `p6m login` to log in.")?;

    if let Some(organization) = organization.clone() {
        device_code_request = device_code_request
            .with_organization(organization)
            .await
            .context("You are not logged in. First run `p6m login` (without --org) to log in.")?
            .with_scope("login:kubernetes");

        token_repository = match device_code_request.exchange_for_token().await {
            Ok(token_repoository) => token_repoository,
            Err(err) => {
                debug!(
                    "Unable to passively get token. Turning on interactive+force: {}",
                    err
                );
                // Switch on force new
                // And allow interactivity
                device_code_request
                    .clone()
                    .interactive(true)
                    .force_new()
                    .with_scope("login:kubernetes")
                    .exchange_for_token()
                    .await?
            }
        };
    }

    let user_info = token_repository.user_info().await?;

    println!(
        "{}",
        match output {
            Some(Output::K8sAuth) =>
                k8s_auth(
                    &token_repository,
                    organization.context("--org is a required for --output k8s-auth")?,
                )
                .await?,
            Some(Output::Json) => user_info.to_json()?,
            None | Some(Output::Default) => user_info.to_string(),
        }
    );

    Ok(())
}

async fn k8s_auth(
    token_repository: &TokenRepository,
    _organization: &String,
    // token_format: TokenFormat,
) -> Result<String, Error> {
    Ok(serde_json::json!({
        "kind": "ExecCredential",
        "apiVersion": "client.authentication.k8s.io/v1beta1",
        "spec": {
            "interactive": false
        },
        "status": {
            "expirationTimestamp": token_repository.clone().read_expiration(AuthToken::Id)?,
            "token": token_repository.clone().read_token(AuthToken::Id)?,
        }
    })
    .to_string())
}
