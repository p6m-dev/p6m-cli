pub mod auth0;
pub mod aws;
pub mod azure;
pub mod vcluster;

use std::fs::create_dir_all;

use anyhow::{Context, Error};
use auth0::configure_auth0;
use aws::configure_aws;
use azure::configure_azure;
use clap::ArgMatches;

use crate::cli::P6mEnvironment;

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    create_dir_all(environment.kube_dir())?;

    let organization = matches
        .try_get_one::<String>("organization-name")
        .unwrap_or(None);

    match matches.subcommand() {
        Some(("auth0", _)) => configure_auth0(&environment, organization)
            .await
            .context("Unable to SSO using Auth0. Please run `p6m login`."),
        Some(("aws", _)) => configure_aws().await,
        Some(("azure", _)) => configure_azure().await,
        Some((command, _)) => Err(Error::msg(format!(
            "Unimplemented sso command: '{}'",
            command
        ))),
        None => configure_sso(&environment).await,
    }?;

    Ok(())
}

async fn configure_sso(_environment: &P6mEnvironment) -> Result<(), Error> {
    // TODO: enable auth0
    // configure_auth0(environment).await?;
    configure_aws().await?;
    configure_azure().await?;
    Ok(())
}
