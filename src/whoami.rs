use crate::auth::AuthToken;
use crate::cli::P6mEnvironment;
use crate::models::openid::DeviceCodeRequest;
use anyhow::{Context, Error};
use clap::ArgMatches;
use log::debug;

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum Output {
    Default,
    Json,
    K8sAws,
}

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    let output = matches
        .try_get_one("output")
        .unwrap_or(Some(&Output::Default));

    let mut device_code_request = DeviceCodeRequest::new(&environment)
        .await?
        .interactive(false);

    let organization = matches.get_one::<String>("organization-name");

    let mut user_info = device_code_request
        .exchange_for_token()
        .await
        .context("You are not logged in. First run `p6m login` to log in.")?
        .user_info()
        .await?;

    if let Some(organization) = organization.clone() {
        device_code_request = device_code_request
            .with_organization(organization)
            .await
            .context("You are not logged in. First run `p6m login` (without --org) to log in.")?;

        let token_repository = match device_code_request.exchange_for_token().await {
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
                    .exchange_for_token()
                    .await?
            }
        };

        user_info = token_repository.user_info().await?;
    }

    println!(
        "{}",
        match output {
            Some(Output::K8sAws) =>
                k8s_auth(
                    device_code_request.clone(),
                    organization.context("--org is a required for k8s-aws auth")?,
                    "k8s-aws-v1.",
                )
                .await?,
            Some(Output::Json) => user_info.to_json()?,
            None | Some(Output::Default) => user_info.to_string(),
        }
    );

    Ok(())
}

async fn k8s_auth(
    device_code_request: DeviceCodeRequest,
    organization: &String,
    token_prefix: &str,
) -> Result<String, Error> {
    let token_repository = device_code_request
        .with_organization(organization)
        .await?
        .with_scope("login:kubernetes")
        .exchange_for_token()
        .await?;

    let token = token_repository.read_token(AuthToken::Access)?;

    Ok(serde_json::json!({
        "kind": "ExecCredential",
        "apiVersion": "client.authentication.k8s.io/v1beta1",
        "spec": {},
        "status": {
            "expirationTimestamp": token_repository.read_expiration(AuthToken::Access)?,
            "token": format!("{}{}", token_prefix, token.context("missing token")?),
        },
    })
    .to_string())
}
