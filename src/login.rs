use crate::{cli::P6mEnvironment, models::openid::DeviceCodeRequest, whoami};
use anyhow::Error;
use clap::ArgMatches;

pub async fn execute(environment: P6mEnvironment, matches: &ArgMatches) -> Result<(), Error> {
    DeviceCodeRequest::new(&environment)
        .await?
        .with_scope("login:cli")
        .exchange_for_token()
        .await?;

    whoami::execute(environment, matches).await
}
