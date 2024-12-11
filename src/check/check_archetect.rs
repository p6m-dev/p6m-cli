use std::process::Command;
use clap::ArgMatches;
use dirs::home_dir;
use crate::check::common::*;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    check_archetect_binary(args)?;
    check_archetect_config(args)?;
    Ok(())
}

fn check_archetect_binary(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("Archetect", Command::new("archetect").arg("--version"), "archetect/#installation")
}

fn check_archetect_config(_args: &ArgMatches) -> anyhow::Result<()> {
    println!("\n{CHECK_PREFIX} Checking Archetect Configuration");
    if !home_dir().expect("Home Directory Required")
        .join(".archetect/etc/archetect.yaml")
        .exists() {
        println!("\t{CHECK_ERROR} Archetect is not configured correctly for your environment.");
        print_see_also("archetect/#configuration");
    } else {
        println!("\t{CHECK_SUCCESS} Archetect Configured");
    }
    Ok(())
}