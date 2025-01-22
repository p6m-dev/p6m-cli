use std::process::Command;
use clap::ArgMatches;
use crate::workstation::check::common::*;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    check_node(args)?;
    check_npm(args)?;
    Ok(())
}

fn check_node(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("NodeJS", Command::new("node").arg("--version"), "javascript/#nodejs")
}

fn check_npm(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("NPM", Command::new("npm").arg("--version"), "javascript/#npm")
}