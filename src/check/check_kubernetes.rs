use crate::check::common::*;
use clap::ArgMatches;
use std::process::Command;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    check_kubectl(args)?;
    check_tilt(args)?;
    check_k9s(args)?;
    Ok(())
}

fn check_kubectl(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check(
        "kubectl",
        Command::new("kubectl").arg("version"),
        "kubernetes/#kubectl",
    )
}

fn check_tilt(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check(
        "Tilt",
        Command::new("tilt").arg("version"),
        "kubernetes/#tilt",
    )
}

fn check_k9s(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("k9s", Command::new("k9s").arg("version"), "kubernetes/#k9s")
}
