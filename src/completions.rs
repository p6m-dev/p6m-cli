use std::io;

use anyhow::Error;
use clap::ArgMatches;
use clap_complete::{generate, Shell};

use crate::cli;

pub fn execute(matches: &ArgMatches) -> Result<(), Error> {
    if let Some(generator) = matches.get_one::<Shell>("generator") {
        let mut cmd = cli::command();
        eprintln!("Generating completion file for {generator}...");
        generate(*generator, &mut cmd, "p6m", &mut io::stdout());
    } else {
        return Err(Error::msg("Invalid completions shell"));
    }

    Ok(())
}
