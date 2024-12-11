use std::process::Command;
use clap::ArgMatches;
use crate::check::common::*;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    check_git_installed(args)?;
    check_git_author(args)?;

    Ok(())
}

pub fn check_git_installed(_args: &ArgMatches) -> anyhow::Result<()> {
    perform_check("Git", Command::new("git").arg("--version"), "scm/#git")
}

pub fn check_git_author(_args: &ArgMatches) -> anyhow::Result<()> {
    println!("\n{CHECK_PREFIX} Checking Git User Name and Email");
    if let Ok(config) = git2::Config::open_default() {
        let name = config.get_string("user.name");
        let email = config.get_string("user.email");

        if let (Ok(name), Ok(email)) = (name, email)  {
            if !name.is_empty() && !email.is_empty() {
                println!("\t{CHECK_SUCCESS} {} <{}>", name, email);
            }
        } else {
            println!("\t{CHECK_ERROR} Git User Name or Email is empty.  Archetypes may use your Git\n\
            User Name and Email to answer questions about code authorship.");

            println!("\n\tExecute the following command to configure git:");
            println!("\n\tgit config --global user.name \"<your name>\"");
            println!("\tgit config --global user.email \"<your email>\"");
        }
    }

    Ok(())
}