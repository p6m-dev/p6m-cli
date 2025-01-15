use crate::auth::{AuthToken, TokenRepository};
use crate::cli::P6mEnvironment;
use anyhow::{Context, Error};
use clap::ArgMatches;

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum Output {
    Default,
    Json,
    K8sAuth,
    AccessToken,
    IdToken,
}

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    let output = matches
        .try_get_one("output")
        .unwrap_or(Some(&Output::Default));

    let organization = matches
        .try_get_one::<String>("organization-name")
        .unwrap_or(None);

    let mut token_repository = TokenRepository::new(&environment)?;

    if let Some(organization) = organization {
        token_repository.with_organization(organization)?;
        token_repository.with_scope("login:kubernetes");
    }

    token_repository.try_login().await?;

    println!(
        "{}",
        match output {
            Some(Output::K8sAuth) =>
                k8s_auth(
                    &token_repository,
                    organization.context("--org is a required for --output k8s-auth")?,
                )
                .await?,
            Some(Output::Json) => token_repository.to_json()?,
            Some(Output::IdToken) => token_repository
                .clone()
                .read_token(AuthToken::Id)
                .context("unable to read id token")?
                .context("missing id token")?,
            Some(Output::AccessToken) => token_repository
                .clone()
                .read_token(AuthToken::Access)
                .context("unable to read id token")?
                .context("missing id token")?,
            None | Some(Output::Default) => token_repository.to_string(),
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
        "spec": {},
        "status": {
            "expirationTimestamp": token_repository.clone().read_expiration(AuthToken::Id)?,
            "token": token_repository.clone().read_token(AuthToken::Id)?,
        }
    })
    .to_string())
}
