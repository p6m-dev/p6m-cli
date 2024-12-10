use crate::auth::{AuthToken, TokenRepository};
use crate::{
    cli::P6mEnvironment,
    login::update_token,
    models::openid::{OpenIdDiscoveryDocument, UserInfo},
};
use anyhow::{Context, Error};
use chrono::NaiveDateTime;
use clap::ArgMatches;
use jsonwebtokens::raw::{self, TokenSlices};
use serde::Deserialize;

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum Output {
    Default,
    Json,
    K8sAuth,
}

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    let token_repository = TokenRepository::new(&environment)?;
    let access_token = token_repository.read_token(AuthToken::Access)?;
    let refresh_token = token_repository.read_token(AuthToken::Refresh)?;

    let output = matches
        .try_get_one("output")
        .unwrap_or(Some(&Output::Default));

    let openid_configuration =
        OpenIdDiscoveryDocument::discover(environment.domain.clone()).await?;

    if let Some(access_token) = access_token {
        let info =
            match UserInfo::request(openid_configuration.userinfo_endpoint.clone(), access_token)
                .await
            {
                Ok(info) => info,
                Err(_) => {
                    match update_token(&openid_configuration, &environment, refresh_token).await {
                        Ok(token_info) => {
                            UserInfo::request(
                                openid_configuration.userinfo_endpoint,
                                token_info.access_token.unwrap(),
                            )
                            .await?
                        }
                        Err(e) => return Err(e),
                    }
                }
            };
        println!(
            "{}",
            match output {
                Some(Output::K8sAuth) => k8s_auth(&token_repository)?,
                Some(Output::Json) => serde_json::to_string_pretty(&info)?,
                None | Some(Output::Default) => format!(
                    "Logged in as: {}",
                    info.email.context("missing email on access token")?
                ),
            }
        );
    } else {
        return Err(anyhow::anyhow!(
            "You are not logged in. Please run `p6m login` to log in."
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct Claims {
    pub exp: i64,
}

fn k8s_auth(token_repository: &TokenRepository) -> Result<String, Error> {
    let token = token_repository.read_token(AuthToken::Access)?;

    let claims = match token.clone() {
        Some(token) => {
            let TokenSlices { claims, .. } =
                raw::split_token(&token).context("unable to split token")?;
            Some(
                serde_json::from_value::<Claims>(
                    raw::decode_json_token_slice(claims).context("unable to decode token")?,
                )
                .context("unable to parse token")?,
            )
        }
        None => None,
    };

    Ok(serde_json::json!({
        "kind": "ExecCredential",
        "apiVersion": "client.authentication.k8s.io/v1beta1",
        "spec": {},
        "status": {
            "expirationTimestamp": match claims {
                Some(claims) => NaiveDateTime::from_timestamp_opt(claims.exp, 0).map(|dt| dt.and_utc()),
                _ => None,
            },
            "token": token,
        },
    })
    .to_string())
}
