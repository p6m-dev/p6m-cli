use std::process::Command;
use clap::ArgMatches;
use dirs::home_dir;
use crate::workstation::check::common::*;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    check_java(args)?;
    check_maven_binary(args)?;
    check_maven_settings(args)?;
    Ok(())
}

pub fn check_java(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("Java", Command::new("java").arg("--version"), "java/#java")
}

pub fn check_maven_binary(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("Maven", Command::new("mvn").arg("--version"), "java/#maven")
}

pub fn check_maven_settings(_args: &ArgMatches) -> anyhow::Result<()> {
    println!("\n{CHECK_PREFIX} Checking Maven Configuration");
    if !home_dir().expect("Home Directory Required")
        .join(".m2/settings.xml")
        .exists() {
        println!("\t{CHECK_ERROR} Maven is not configured correctly for your environment.");
        print_see_also("java/#maven");
    } else {
        println!("\t{CHECK_SUCCESS} Maven Configured");
    }
    Ok(())
}