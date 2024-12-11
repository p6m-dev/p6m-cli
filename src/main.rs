extern crate clap;

mod cli;
mod check;
mod completions;
mod context;
mod logging;
mod login;
mod models;
mod open;
mod purge;
mod repositories;
mod sso;
mod tilt;
mod version;
mod whoami;
mod auth;

use cli::P6mEnvironment;
use log::error;

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
        Some(("check", subargs)) => check::execute(subargs),
        Some(("context", subargs)) => context::execute(subargs).await,
        Some(("open", subargs)) => open::execute(subargs).await,
        Some(("purge", subargs)) => purge::execute(subargs),
        Some(("repositories", subargs)) => repositories::execute(subargs).await,
        Some(("tilt", subargs)) => tilt::execute(subargs).await,
        Some(("sso", subargs)) => sso::execute(subargs).await,
        Some(("login", subargs)) => login::execute(environment, subargs).await,
        Some(("whoami", subargs)) => whoami::execute(environment, subargs).await,
        Some((command, _)) => Err(anyhow::Error::msg(format!("Invalid command: {command}"))),
        None => Err(anyhow::Error::msg("No command given")),
    };

    if let Err(e) = result {
        error!("{}", e);
    }
}
