use crate::auth::{Claims, TokenRepository, TryReason};
use crate::cli::P6mEnvironment;
use crate::AuthToken;
use anyhow::{Context, Error};
use chrono::{DateTime, Utc};
use clap::ArgMatches;
use log::debug;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum Output {
    Default,
    Json,
    K8sAuth,
    AccessToken,
    IdToken,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct K8sAuth {
    pub kind: Option<String>,
    pub api_version: Option<String>,
    pub spec: Option<K8sAuthSpec>,
    pub status: Option<K8sAuthStatus>,
}

impl Default for K8sAuth {
    fn default() -> Self {
        Self {
            kind: Some("ExecCredential".into()),
            api_version: Some("client.authentication.k8s.io/v1beta1".into()),
            spec: Some(K8sAuthSpec {
                interactive: Some(false),
            }),
            status: Some(K8sAuthStatus {
                expiration_timestamp: None,
                token: None,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct K8sAuthSpec {
    pub interactive: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct K8sAuthStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_timestamp: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
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
        .try_refresh(&TryReason::WhoAmICommand)
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
            token_repository
                .force()
                .try_login(&TryReason::WhoAmICommand)
                .await?
        }
    };

    match (output, authn_app_id) {
        (Some(Output::K8sAuth), Some(authn_app_id)) => {
            // Skip re-authenticating if kuberlr is resolving the version
            if !env::var("KUBERLR_RESOLVING_VERSION").is_ok() {
                token_repository = token_repository
                    .with_authn_app_id(authn_app_id)
                    .await
                    .context(format!("Unable to authenticate"))?;
            }
        }
        _ => {}
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
    let auth = K8sAuth {
        kind: Some("ExecCredential".into()),
        api_version: Some("client.authentication.k8s.io/v1beta1".into()),
        spec: Some(K8sAuthSpec {
            interactive: Some(false),
        }),
        status: Some(K8sAuthStatus {
            expiration_timestamp: token_repository
                .clone()
                .read_expiration(
                    token_repository
                        .auth_n
                        .clone()
                        .token_preference
                        .unwrap_or(AuthToken::Id),
                )
                .ok(),
            token: token_repository.clone().read_token(
                token_repository
                    .auth_n
                    .clone()
                    .token_preference
                    .unwrap_or(AuthToken::Id),
            )?,
        }),
    };

    Ok(serde_json::json!(auth).to_string())
}
