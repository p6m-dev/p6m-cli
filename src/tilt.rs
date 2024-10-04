use anyhow::Error;
use clap::ArgMatches;
use inquire::MultiSelect;
use log::{info, warn};
use minijinja::render;

use crate::models::git::GithubLevel;
use crate::models::git::Repository;

pub async fn execute(matches: &ArgMatches) -> Result<(), Error> {
    match matches.subcommand() {
        Some(("generate", subargs)) => generate(subargs).await,
        Some((command, _)) => Err(Error::msg(format!("Unimplemented tilt command: '{}'", command))),
        None => Err(Error::msg("Unspecified tilt command".to_owned())),
    }?;

    Ok(())
}

async fn generate(_matches: &ArgMatches) -> Result<(), Error> {
    let org_path = GithubLevel::current()?;

    if let Some(organization) = org_path.organization() {
        let repositories = organization.repositories()?
            .filter(|repo| repo.has_path("Tiltfile"))
            .collect::<Vec<Repository>>();

        if let Ok(selected_repositories) = MultiSelect::new("Applications to include:", repositories)
            .with_page_size(25)
            .prompt() {

            let applications = selected_repositories.iter()
                .map(|repo| repo.name().to_owned())
                .collect::<Vec<String>>();

            if applications.is_empty() {
                let tiltfile_contents = render!(include_str!("../resources/Tiltfile"),
                    applications
                );
                let mut tiltfile_path = organization.local_path();
                tiltfile_path.push("Tiltfile");
                tokio::fs::write(tiltfile_path, tiltfile_contents).await?;
                info!("Tiltfile written.  Execute 'tilt up' within {:?}", organization.local_path());
            } else {
                warn!("No applications selected. Titlefile not written")
            }
        }
    }
    Ok(())
}