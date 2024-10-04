use anyhow::Error;
use clap::ArgMatches;

use crate::{
    cli::YborEnvironment,
    login::update_token,
    models::openid::{OpenIdDiscoveryDocument, UserInfo},
};
use crate::auth::{TokenRepository, AuthToken};

pub async fn execute(environment: YborEnvironment, _matches: &ArgMatches) -> Result<(), Error> {
    let token_repository = TokenRepository::new(&environment)?;
    let access_token = token_repository.read_token(AuthToken::Access)?;
    let refresh_token = token_repository.read_token(AuthToken::Refresh)?;

    let openid_configuration =
        OpenIdDiscoveryDocument::discover(environment.domain.clone()).await?;


    if let Some(access_token) = access_token {
        let info =
            match UserInfo::request(openid_configuration.userinfo_endpoint.clone(), access_token).await {
                Ok(info) => info,
                Err(_) => match update_token(
                    &openid_configuration,
                    &environment,
                    refresh_token,
                )
                .await
                {
                    Ok(token_info) => {
                        UserInfo::request(
                            openid_configuration.userinfo_endpoint,
                            token_info.access_token.unwrap(),
                        )
                        .await?
                    }
                    Err(e) => return Err(e),
                },
            };
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("You are not logged in. Please run `ybor login` to log in.");
    }
    Ok(())
}
