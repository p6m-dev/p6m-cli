use std::process::Command;
use clap::ArgMatches;
use crate::check::common::*;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    check_docker(args)?;
    Ok(())
}

fn check_docker(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("Docker", Command::new("docker").arg("--version"), "core/docker/")
}