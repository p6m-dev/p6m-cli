use crate::{auth::TokenRepository, cli::P6mEnvironment, whoami};
use anyhow::Error;
use clap::ArgMatches;

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    let organization = matches
        .try_get_one::<String>("organization-name")
        .unwrap_or(None);

    let mut token_repository = TokenRepository::new(&environment)?.force();

    if let Some(organization) = organization {
        token_repository.with_organization(organization)?;
    }

    token_repository.try_login().await?;

    println!("\nLogged in!\n");
    whoami::execute(environment, matches).await
}
