use std::process::Command;
use clap::ArgMatches;
use crate::workstation::check::common::*;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    check_dotnet_binary(args)?;
    Ok(())
}

pub fn check_dotnet_binary(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("dotnet", Command::new("dotnet").arg("--version"), "dotnet/")
}