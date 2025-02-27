use crate::{
    auth::{TokenRepository, TryReason},
    cli::P6mEnvironment,
    whoami,
};
use anyhow::{Context, Error};
use clap::ArgMatches;

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    let organization = matches
        .try_get_one::<String>("organization-name")
        .unwrap_or(None);

    let refresh = matches.try_get_one::<bool>("refresh").unwrap_or(None);

    let mut token_repository = TokenRepository::new(&environment.auth_n, &environment.auth_dir)?;
    token_repository.force();

    if let Some(organization) = organization {
        token_repository.with_organization(organization)?;
    }

    match refresh {
        Some(true) => token_repository
            .try_refresh(&TryReason::LoginCommand)
            .await
            .context("Please re-run `p6m login`")?,
        _ => token_repository
            .try_login(&TryReason::LoginCommand)
            .await
            .context("Please re-run `p6m login`")?,
    };

    println!("\nLogged in!\n");
    whoami::execute(environment, matches).await
}
