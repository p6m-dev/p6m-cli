pub mod aws;
pub mod azure;

use anyhow::Error;
use aws::configure_aws;
use azure::configure_azure;
use clap::ArgMatches;

pub async fn execute(matches: &ArgMatches) -> Result<(), Error> {
    match matches.subcommand() {
        Some(("aws", _)) => configure_aws().await,
        Some(("azure", _)) => configure_azure().await,
        Some((command, _)) => Err(Error::msg(format!(
            "Unimplemented sso command: '{}'",
            command
        ))),
        None => configure_sso().await,
    }?;

    Ok(())
}

async fn configure_sso() -> Result<(), Error> {
    configure_aws().await?;
    configure_azure().await?;
    Ok(())
}
