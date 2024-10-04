use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use anyhow::Error;
use dirs::home_dir;
use itertools::Itertools;
use serde::Serialize;

pub enum GithubLevel {
    Enterprise,
    Organization(Organization),
    Repository(Repository),
}

impl GithubLevel {
    pub fn current() -> Result<GithubLevel, Error> {
        GithubLevel::from_path(std::env::current_dir()?)
    }

    pub fn from_path<PATH: AsRef<Path>>(path: PATH) -> Result<GithubLevel, Error> {
        if let Ok(org_path) = path.as_ref().strip_prefix(orgs_root()) {
            let path_elements: Vec<String> = org_path.components()
                .map(|c| c.as_os_str().to_str().unwrap().to_string())
                .collect();

            let org_path = match path_elements.len() {
                0 => GithubLevel::Enterprise,
                1 => GithubLevel::Organization(Organization::new(path_elements[0].clone())),
                _ => GithubLevel::Repository(Repository::new(path_elements[0].clone(), path_elements[1].clone())),
            };

            return Ok(org_path);
        }

        Err(Error::msg("You must be within your local ~/orgs/ directory.".to_owned()))
    }

    pub fn with_organization(organization_name: Option<&String>) -> Result<GithubLevel, Error> {
        if let Some(org) = organization_name {
            return Ok(GithubLevel::Organization(Organization::new(org.to_owned())));
        } else {
            let org_path = GithubLevel::current()?;
            match org_path {
                GithubLevel::Organization { .. } => {
                    return Ok(org_path);
                }
                GithubLevel::Repository { .. } => {
                    return Ok(org_path);
                }
                _ => (),
            }
        }

        Err(Error::msg("You must be within an organization or repository directory, or specify an organization as an argument"))
    }

    pub fn github_url(&self) -> String {
        match self {
            GithubLevel::Enterprise => "https://github.com/enterprises/ybor".to_owned(),
            GithubLevel::Organization(organization) => { format!("https://github.com/{}", organization) }
            GithubLevel::Repository(repository) => { format!("https://github.com/{}", repository) }
        }
    }

    pub fn local_path(&self) -> PathBuf {
        match self {
            GithubLevel::Enterprise => {
                orgs_root()
            }
            GithubLevel::Organization(organization) => {
                org_directory(organization.name())
            }
            GithubLevel::Repository(repository) => {
                let mut repo_directory = org_directory(repository.organization().name());
                repo_directory.push(repository.name());
                repo_directory
            }
        }
    }

    pub fn organization(&self) -> Option<Organization> {
        match self {
            GithubLevel::Organization(organization) => {
                Some(organization.clone())
            }
            GithubLevel::Repository(repository) => {
                Some(repository.organization())
            }
            _ => None,
        }
    }

    pub fn repository(&self) -> Option<Repository> {
        match self {
            GithubLevel::Repository(repository) => {
                Some(repository.clone())
            }
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Organization {
    name: String,
}

impl Organization {
    pub fn new<NAME: Into<String>>(name: NAME) -> Organization {
        Organization { name: name.into() }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn org_path(&self) -> GithubLevel {
        GithubLevel::Organization(self.clone())
    }

    pub fn local_path(&self) -> PathBuf {
        self.org_path().local_path()
    }

    pub fn repositories(&self) -> Result<impl Iterator<Item=Repository> + '_, Error> {
        let iter = std::fs::read_dir(self.local_path())?
            .filter(|result| result.is_ok())
            .map(|result| result.unwrap().path())
            .filter(|path| path.is_dir())
            .filter(|path| path.file_name().is_some())
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .map(|repo_name| Repository::new(self.name(), repo_name))
            .sorted()
            ;
        Ok(iter)
    }
}

impl Display for Organization {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Clone, Eq, PartialOrd, PartialEq, Ord, Serialize)]
pub struct Repository {
    org: String,
    name: String,
    private: bool,
    has_wiki: bool,
    has_issues: bool,
}

impl Repository {
    pub fn new<ORG: Into<String>, REPO: Into<String>>(org: ORG, repo: REPO) -> Repository {
        Repository { org: org.into(), name: repo.into(), private: true, has_wiki: false, has_issues: false }
    }

    pub fn organization(&self) -> Organization {
        Organization { name: self.org.clone() }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn org_path(&self) -> GithubLevel {
        GithubLevel::Repository(self.clone())
    }

    pub fn local_path(&self) -> PathBuf {
        self.org_path().local_path()
    }

    pub fn has_path(&self, path: &str) -> bool {
        let mut candidate_path = self.local_path();
        candidate_path.push(path);
        candidate_path.exists()
    }
}

impl Display for Repository {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.organization().name(), self.name())
    }
}

pub fn orgs_root() -> PathBuf {
    let mut root = home_dir().expect("Error locating home directory");
    root.push("orgs");
    root
}

pub fn org_directory(org: &str) -> PathBuf {
    let mut result = orgs_root();
    result.push(org);
    result
}