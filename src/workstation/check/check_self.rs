use clap::ArgMatches;
use log::error;
use crate::version::current_version;
use crate::workstation::check::common::*;

pub async fn execute(_args: &ArgMatches) -> anyhow::Result<()> {
    println!("\n{CHECK_PREFIX} Checking p6m CLI Version");
    let octocrab = crate::repositories::create_octocrab()?;
    match octocrab.repos("p6m-dev", "p6m-cli").releases().get_latest().await {
        Ok(release) => {
            let latest_version = release.tag_name;
            let current_version = format!("v{}", current_version().version());
            if latest_version == current_version {
                println!("\t{CHECK_SUCCESS} {latest_version}");
            } else {
                println!("\t{CHECK_WARN} The current version of the p6m CLI is {current_version}, but {latest_version} is available.");
                print_see_also("core/p6m-cli");
            }
        }
        Err(error) => {
            error!("Failure checking p6m-cli version: {error}");
        }
    }
    Ok(())
}