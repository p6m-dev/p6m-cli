use crate::{cli::P6mEnvironment, models::openid::DeviceCodeRequest, whoami};
use anyhow::{Context, Error};
use clap::ArgMatches;

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    let mut device_code_request = DeviceCodeRequest::new(&environment).await?.force_new();

    if let Some(organization) = matches.get_one::<String>("organization-name") {
        device_code_request = device_code_request
            .with_organization(organization)
            .await
            .context("You are not logged in. First run `p6m login` (without --org) to log in.")?
            .force_new();
    }

    device_code_request.exchange_for_token().await?;

    println!("\nLogged in!\n");
    whoami::execute(environment, matches).await
}
