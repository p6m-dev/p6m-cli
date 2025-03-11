extern crate clap;

mod auth;
mod auth0;
mod cli;
mod completions;
mod context;
mod jwt;
mod logging;
mod login;
mod models;
mod open;
mod purge;
mod repositories;
mod sso;
mod tilt;
mod whoami;
mod workstation;

use cli::P6mEnvironment;
use log::error;

pub use auth0::*;

#[tokio::main]
async fn main() {
    let matches = cli::command().get_matches();
    logging::init(&matches);
    let environment = match P6mEnvironment::init(&matches) {
        Ok(environment) => environment,
        Err(e) => {
            error!("{}", e);
            return;
        }
    };

    let result = match matches.subcommand() {
        Some(("completions", subargs)) => completions::execute(subargs),
        Some(("context", subargs)) => context::execute(subargs).await,
        Some(("open", subargs)) => open::execute(subargs).await,
        Some(("purge", subargs)) => purge::execute(subargs),
        Some(("repositories", subargs)) => repositories::execute(subargs).await,
        Some(("jwt", subargs)) => jwt::execute(environment, subargs).await,
        Some(("tilt", subargs)) => tilt::execute(subargs).await,
        Some(("sso", subargs)) => sso::execute(environment, subargs).await,
        Some(("login", subargs)) => login::execute(environment, subargs).await,
        Some(("whoami", subargs)) => whoami::execute(environment, subargs).await,
        Some(("workstation", subargs)) => workstation::execute(subargs).await,
        Some((command, _)) => Err(anyhow::Error::msg(format!("Invalid command: {command}"))),
        None => Err(anyhow::Error::msg("No command given")),
    };

    if let Err(e) = result {
        error!(
            "{}",
            e.chain()
                .map(|e| e.to_string())
                .collect::<Vec<String>>()
                .join(": ")
        );
    }
}
