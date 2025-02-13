use crate::auth::{Claims, TokenRepository};
use crate::cli::P6mEnvironment;
use crate::AuthToken;
use anyhow::{Context, Error};
use clap::ArgMatches;
use log::debug;

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

    let authn_app_id = matches
        .try_get_one::<String>("authn-app-id")
        .unwrap_or(None);

    let mut token_repository = TokenRepository::new(&environment.auth_n, &environment.auth_dir)?;

    if let Some(organization) = organization {
        if output == Some(&Output::K8sAuth) {
            token_repository.with_scope(
                "login:kubernetes",
                Claims {
                    login_kubernetes: Some("true".into()),
                    ..Default::default()
                },
            );
        }
        token_repository
            .with_organization(organization)
            .context("Unknown organizatization")?;
    }

    token_repository = match token_repository
        .try_refresh()
        .await
        .map_err(|e| {
            debug!("Unable to refresh: {}", e);
            e
        })
        .ok()
    {
        Some(token_repository) => token_repository,
        None => {
            // TODO
            debug!("Unable to refresh, trying to login");
            token_repository.force().try_login().await?
        }
    };

    if let Some(id) = authn_app_id {
        token_repository = token_repository
            .with_authn_app_id(id)
            .await
            .context(format!("Unable to authenticate"))?;
    }

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
) -> Result<String, Error> {
    Ok(serde_json::json!({
        "kind": "ExecCredential",
        "apiVersion": "client.authentication.k8s.io/v1beta1",
        "spec": {
            "interactive": true,
        },
        "status": {
            "expirationTimestamp": token_repository.clone().read_expiration(
                token_repository
                    .auth_n
                    .clone()
                    .token_preference
                    .unwrap_or(AuthToken::Id),
            )?,
            "token": token_repository.clone().read_token(
                token_repository
                    .auth_n
                    .clone()
                    .token_preference
                    .unwrap_or(AuthToken::Id),
            )?,
        }
    })
    .to_string())
}
