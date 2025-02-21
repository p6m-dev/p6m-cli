use anyhow::{Context, Error};
use clap::ArgMatches;
use inquire::{Confirm, MultiSelect};
use log::{error, info, warn};
use octocrab::models::orgs::Organization;
use octocrab::{Octocrab, Page};
use serde::Serialize;
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;

use crate::models::git::{org_directory, GithubLevel, Repository};

pub async fn execute(matches: &ArgMatches) -> Result<(), Error> {
    match matches.subcommand() {
        Some(("pull", subargs)) => pull(subargs).await,
        Some(("push", subargs)) => push(subargs).await,
        Some(("delete", subargs)) => delete(subargs).await,
        Some((command, _)) => Err(Error::msg(format!(
            "Unimplemented repos command: '{}'",
            command
        ))),
        None => Err(Error::msg("No repo command given")),
    }?;

    Ok(())
}

async fn pull(matches: &ArgMatches) -> Result<(), Error> {
    let client = create_octocrab()?;

    if let Some(org_name) = matches.get_one::<String>("organization-name") {
        pull_organization(&client, matches, org_name).await?
    } else if let Ok(org_path) = GithubLevel::current() {
        match org_path {
            GithubLevel::Enterprise => pull_organizations(&client, matches).await?,
            GithubLevel::Organization(organization) => {
                pull_organization(&client, matches, organization.name()).await?
            }
            GithubLevel::Repository(repository) => {
                pull_organization(&client, matches, repository.organization().name()).await?
            }
        }
    } else {
        pull_organizations(&client, matches).await?
    }

    Ok(())
}

async fn pull_organizations(client: &Octocrab, matches: &ArgMatches) -> Result<(), Error> {
    let org_first_page = client.list_orgs().await?;

    let orgs: Vec<Organization> = client
        .all_pages(org_first_page)
        .await?
        .into_iter()
        .filter(|org| org.login != "p6m-dev") // Skip p6m-dev
        .collect();

    for org in orgs {
        pull_organization(client, matches, &org.login).await?;
    }

    Ok(())
}

async fn pull_organization(
    client: &Octocrab,
    matches: &ArgMatches,
    org_name: &str,
) -> Result<(), Error> {
    let dry_run = matches.get_flag("dry-run");
    let all = matches.get_flag("all");
    let _prune = matches.get_flag("prune");

    let org_directory = org_directory(org_name);
    fs::create_dir_all(&org_directory).await?;

    let repos_first_page = client
        .orgs(org_name)
        .list_repos()
        .repo_type(octocrab::params::repos::Type::All)
        .per_page(25)
        .send()
        .await?;

    let repos = client.all_pages(repos_first_page).await?;

    for repo in &repos {
        let repository = Repository::new(org_name, &repo.name);

        if !repository.local_path().exists() {
            info!("Cloning {}", repository);
            if !dry_run {
                let result = Command::new("git")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .arg("-C")
                    .arg(repository.local_path().parent().unwrap())
                    .arg("clone")
                    .arg(repo.ssh_url.as_ref().unwrap())
                    .arg(repository.local_path())
                    .status()
                    .await;

                match result {
                    Ok(code) => match code.code() {
                        Some(code) if code != 0 => {
                            let cmd = format!(
                                "git -C {:?} clone {:?} {:?}",
                                repository.local_path().parent().unwrap(),
                                &repo.ssh_url.as_ref().unwrap(),
                                repository.local_path()
                            );
                            error!("Error cloning {:?}: Code {}. Try running command directly for more detailed error message. {}", repository.local_path(), code, cmd);
                        }
                        _ => {}
                    },
                    Err(err) => {
                        error!("Error cloning {:?}: {}", repository.local_path(), err);
                    }
                }
            }
        } else if all {
            info!("Pulling {}", repository);
            if !dry_run {
                let result = Command::new("git")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .arg("-C")
                    .arg(repository.local_path())
                    .arg("pull")
                    .status()
                    .await;
                match result {
                    Ok(code) => match code.code() {
                        Some(code) if code != 0 => {
                            error!("Error pulling {:?}: Code {}", repository.local_path(), code);
                        }
                        _ => {}
                    },
                    Err(err) => {
                        error!("Error pulling {:?}: {}", repository.local_path(), err);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn push(matches: &ArgMatches) -> Result<(), Error> {
    let dry_run = matches.get_flag("dry-run");
    let all = matches.get_flag("all");
    let org_path = GithubLevel::current()?;

    if let Some(repository) = org_path.repository() {
        let confirmed = Confirm::new(&format!(
            "Are you sure you want to push {}?",
            org_path.github_url()
        ))
        .with_default(true)
        .prompt()?;

        if confirmed {
            push_repository(&repository, dry_run).await?;
        }
    } else if let Some(organization) = org_path.organization() {
        let repos = organization
            .repositories()?
            .filter(|repo| all || !repo.has_path(".git"))
            .collect::<Vec<Repository>>();

        if let Ok(selected_repositories) = MultiSelect::new("Repos to push:", repos)
            .with_page_size(25)
            .prompt()
        {
            let confirmed = Confirm::new("Are you sure you want to push these directories?")
                .with_default(false)
                .prompt()?;

            if confirmed {
                for repository in selected_repositories {
                    push_repository(&repository, dry_run).await?;
                }
            }
        } else {
            info!("No repositories to push");
        }
    } else {
        return Err(Error::msg("You must be within an organization or repository within ~/orgs/ for this command to work."));
    }

    Ok(())
}

async fn push_repository(repository: &Repository, dry_run: bool) -> Result<(), Error> {
    info!("Creating {}", repository.org_path().github_url());

    let octocrab = create_octocrab()?;
    let org_path = repository.org_path();

    if !dry_run {
        let create_repository = OrgRepository::from(repository.clone());
        match octocrab.create_org_repo(&create_repository).await {
            Ok(_) => {}
            Err(_) => warn!(
                "Error creating {}.  It may already exist.",
                org_path.github_url()
            ),
        }
    }

    if !repository.has_path(".git") {
        info!("Initializing {}", repository);
        if !dry_run {
            Command::new("git")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .arg("-C")
                .arg(&repository.local_path())
                .arg("init")
                .status()
                .await?;
            Command::new("git")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .arg("-C")
                .arg(&repository.local_path())
                .arg("add")
                .arg(".")
                .status()
                .await?;
            Command::new("git")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .arg("-C")
                .arg(&repository.local_path())
                .arg("commit")
                .arg("-m")
                .arg("initial commit")
                .status()
                .await?;
            Command::new("git")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .arg("-C")
                .arg(&repository.local_path())
                .arg("remote")
                .arg("add")
                .arg("origin")
                .arg(format!(
                    "git@github.com:{organization}/{repository}.git",
                    organization = repository.organization().name(),
                    repository = repository.name()
                ))
                .status()
                .await?;
        }
    }
    info!("Pushing {}", repository);
    if !dry_run {
        Command::new("git")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .arg("-C")
            .arg(&repository.local_path())
            .arg("push")
            .arg("-u")
            .arg("origin")
            .arg("HEAD")
            .status()
            .await?;
    }

    Ok(())
}

async fn delete(matches: &ArgMatches) -> Result<(), Error> {
    let dry_run = matches.get_flag("dry-run");
    let octocrab = create_octocrab()?;

    if dry_run {
        warn!("Dry run mode... nothing will actually be deleted");
    }

    if let Ok(org_path) = &GithubLevel::current() {
        if !(allow_deletes(org_path)) {
            return Err(Error::msg(
                "Repositories can only be deleted from 'example' organizations",
            ));
        }
        match org_path {
            GithubLevel::Repository(repository) => {
                let confirmed = Confirm::new(&format!("Are you sure you want to delete {}?", org_path.github_url()))
                    .with_default(false)
                    .prompt()?;

                if confirmed {
                    warn!("Deleting {}", org_path.github_url());
                    if !dry_run {
                        octocrab.repos(repository.organization().name(), repository.name())
                            .delete()
                            .await?;
                    }
                }
            }
            GithubLevel::Organization(organization) => {
                let repos = organization.repositories()?
                    .collect::<Vec<Repository>>();

                if let Ok(selected_repositories) = MultiSelect::new("Remote repos to delete:", repos)
                    .with_page_size(20)
                    .prompt() {
                    let confirmed = Confirm::new("Are you sure you want to delete these remote repositories?")
                        .with_default(false)
                        .prompt()?;

                    if confirmed {
                        for repository in selected_repositories {
                            warn!("Deleting {}", repository.org_path().github_url());
                            if !dry_run {
                                match octocrab.repos(repository.organization().name().to_string(), repository.name().to_string())
                                    .delete()
                                    .await {
                                    Ok(_) => {}
                                    Err(err) => warn!("{}", err)
                                }
                            }
                        }
                    }
                }
            }
            _ => return Err(Error::msg("You must be within an organization or repository within ~/orgs/ for this command to work."))
        }
    }

    Ok(())
}

fn allow_deletes(org_path: &GithubLevel) -> bool {
    match org_path {
        GithubLevel::Organization(organization) => {
            organization.name().contains("example") || organization.name().contains("playstation")
        }
        GithubLevel::Repository(repository) => repository.organization().name().contains("example"),
        _ => false,
    }
}

pub(crate) fn create_octocrab() -> Result<Octocrab, Error> {
    let token = std::env::var("GITHUB_TOKEN").context(
        "GITHUB_TOKEN env variable must be set with a classic personal token.\n\n
            See {DOCS_PREFIX}:",
    )?;

    let client = Octocrab::builder().personal_token(token).build()?;
    Ok(client)
}

#[async_trait::async_trait]
trait OctocrabExtensions {
    async fn list_orgs(&self) -> octocrab::Result<Page<Organization>>;
    // async fn create_repo(&self, org: String, repo: String) -> octocrab::Result<()>;
    async fn create_org_repo(&self, repository: &OrgRepository) -> octocrab::Result<()>;
}

#[async_trait::async_trait]
impl OctocrabExtensions for Octocrab {
    async fn list_orgs(&self) -> octocrab::Result<Page<Organization>> {
        self.get("/user/orgs", None::<&()>).await
    }

    // async fn create_repo(&self, org: String, repo: String) -> octocrab::Result<()> {
    //     let repository = Repository::new(org.clone(), repo);

    //     let _response: octocrab::models::Repository = self
    //         .post(format!("/orgs/{}/repos", org), Some(&repository))
    //         .await?;

    //     Ok(())
    // }

    async fn create_org_repo(&self, repository: &OrgRepository) -> octocrab::Result<()> {
        let _response: octocrab::models::Repository = self
            .post(
                format!("/orgs/{}/repos", &repository.org),
                Some(&repository),
            )
            .await?;

        Ok(())
    }
}

#[derive(Clone, Eq, PartialOrd, PartialEq, Ord, Serialize)]
pub struct OrgRepository {
    org: String,
    name: String,
    private: bool,
    has_wiki: bool,
    has_issues: bool,
    visibility: String,
}

impl From<Repository> for OrgRepository {
    fn from(value: Repository) -> Self {
        OrgRepository {
            org: value.organization().name().to_string(),
            name: value.name().to_string(),
            private: false,
            has_wiki: false,
            has_issues: false,
            visibility: "private".to_string(),
        }
    }
}
