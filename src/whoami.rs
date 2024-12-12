use crate::auth::AuthToken;
use crate::cli::P6mEnvironment;
use crate::models::openid::DeviceCodeRequest;
use anyhow::{Context, Error};
use clap::ArgMatches;

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

    let device_code_request = DeviceCodeRequest::new(&environment)
        .await?
        .interactive(false);

    let user_info = device_code_request
        .exchange_for_token()
        .await?
        .user_info()
        .await
        .context("You are not logged in. Please run `p6m login` to log in.")?;

    println!(
        "{}",
        match output {
            Some(Output::K8sAuth) =>
                k8s_auth(
                    device_code_request.clone(),
                    matches
                        .try_get_one::<String>("organization-name")
                        .context("missing organization-name option")?
                        .context("--org is a required for k8s-auth")?
                )
                .await?,
            Some(Output::Json) => serde_json::to_string_pretty(&user_info)?,
            None | Some(Output::Default) => format!(
                "Logged in as: {}",
                user_info.email.context("missing email on access token")?
            ),
        }
    );

    Ok(())
}

async fn k8s_auth(
    device_code_request: DeviceCodeRequest,
    organization: &String,
) -> Result<String, Error> {
    let token_repository = device_code_request
        .interactive(true)
        .with_organization(organization)
        .await?
        .without_scope("login:cli")
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
            "token": token.context("missing token")?,
        },
    })
    .to_string())
}
