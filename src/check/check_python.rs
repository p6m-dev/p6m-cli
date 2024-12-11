use std::process::Command;
use clap::ArgMatches;
use crate::check::common::*;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    check_python(args)?;
    check_pip(args)?;
    Ok(())
}

fn check_python(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("Python", Command::new("python3").arg("--version"), "python/#python")
}

fn check_pip(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("PIP", Command::new("pip3").arg("--version"), "python/#pip")
}