use anyhow::Error;
use clap::ArgMatches;

use crate::models::git::GithubLevel;

pub async fn execute(matches: &ArgMatches) -> Result<(), Error> {
    match matches.subcommand() {
        Some(("argocd", subaqrgs)) => argocd_page(subaqrgs).await,
        Some(("artifactory", subargs)) => artifactory_page(subargs).await,
        Some(("github", _)) => github_page().await,
        Some((command, _)) => Err(Error::msg(format!(
            "Unimplemented repos command: '{}'",
            command
        ))),
        None => Err(Error::msg("Unspecified repos command")),
    }?;

    Ok(())
}

async fn github_page() -> Result<(), Error> {
    let org_path = GithubLevel::current()?;
    webbrowser::open(&org_path.github_url())?;
    Ok(())
}

async fn argocd_page(matches: &ArgMatches) -> Result<(), Error> {
    let organization_name = GithubLevel::with_organization(matches.get_one("organization"))?
        .organization()
        .unwrap()
        .name()
        .to_string();

    webbrowser::open(&format!(
        "https://{}-argocd.run-studio.p6m.run/applications",
        organization_name
    ))
    .map(|_| ())
    .map_err(|err| err.into())
}

async fn artifactory_page(matches: &ArgMatches) -> Result<(), Error> {
    let organization_name = GithubLevel::with_organization(matches.get_one("organization"))?
        .organization()
        .unwrap()
        .name()
        .to_string();
    webbrowser::open(&format!(
        "https://ybor.jfrog.io/ui/packages?projectKey={}",
        organization_name
    ))
    .map(|_| ())
    .map_err(|err| err.into())
}
