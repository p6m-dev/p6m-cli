use crate::models::artifact;
use crate::whoami;
use crate::workstation::check::Ecosystem;
use crate::{AuthN, AuthToken};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{crate_version, value_parser, Arg, ArgMatches, Command};
use clap_complete::Shell;
use std::fs::create_dir_all;

pub fn command() -> Command {
    clap::command!()
        .name("") // this string is prepended to -V and --version, resulting in invalid json
        .author("P6m Dev")
        .version(crate_version!())
        .about("p6m CLI")
        .subcommand(
            Command::new("completions")
                .about("Generate shell completions")
                .arg_required_else_help(true)
                .arg(
                    Arg::new("generator")
                        .value_parser(value_parser!(Shell)),
                )
        )
        .subcommand(
            Command::new("workstation")
                .about("Workstation Checks and Setup")
                .alias("ws")
                .subcommand(Command::new("check")
                    .about("Workstation Checks")
                    .arg(
                        Arg::new("ecosystem")
                            .value_parser(value_parser!(Ecosystem))
                            .required(false)
                            .action(clap::ArgAction::Append)
                            .help("Ecosystem to check")
                    )
                )
                .subcommand(
                    Command::new("setup")
                        .about("Workstation Setups")
                )
        )
        .subcommand(Command::new("context")
            .about("Switch Organization Contexts")
            .arg(
                Arg::new("organization-name")
                    .long("org")
                    .short('o')
                    .required(false)
                    .action(clap::ArgAction::Set)
                    .help("The JV Organization Name")
            )
            .arg(
                Arg::new("provider")
                    .long("provider")
                    .short('p')
                    .required(false)
                    .value_parser(value_parser!(artifact::StorageProvider))
                    .help("The storage provider to activate for this context.")
            )
        )
        .subcommand(Command::new("jwt")
            .about("Generate JWTs") 
            .subcommand(Command::new("unsecured")
                .about("Generates an UNSECURED JWT for development")
                .alias("u")
            )
        )
        .subcommand(Command::new("open")
            .about("Open an Organization Resource")
            .arg_required_else_help(true)
            .subcommand(
                Command::new("github")
                    .visible_alias("gh")
                    .about("Opens Github to the corresponding local repository, organization, or enterprise.")
            )
            .subcommand(
                Command::new("argocd")
                    .visible_aliases(["argo", "acd"])
                    .about("Opens ArgoCD to the corresponding local repository or organization")
                    .arg(
                        Arg::new("environment")
                            .value_parser(value_parser!(Environment))
                            .default_value("dev")
                            .required(false),
                    ),
            )
            .subcommand(
                Command::new("artifactory")
                    .visible_alias("af")
                    .about("Opens Artifactory to the corresponding local repository or organization")
            )
        )
        .subcommand(
            Command::new("purge")
                .about("Purge Commands")
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("ide-files")
                        .about("Purges IDE files recursively within one or more projects.")
                        .arg(Arg::new("dry-run").long("dry-run").action(clap::ArgAction::SetTrue)),
                )
                .subcommand(
                    Command::new("maven")
                        .about("Purge subsets of the local Maven cache")
                        .arg(
                            Arg::new("path")
                                .required(true)
                                .help(
                                    "Specifies the path to purge as a subset of the Maven coordinates.",
                                ),
                        ),
                ),
        )
        .subcommand(Command::new("repositories")
            .visible_aliases(["repos", "repo"])
            .about("Operations on Organization repos")
            .subcommand(Command::new("pull")
                .about("Pull repos for one or more organizations")
                .arg(
                    Arg::new("organization-name")
                        .long("org")
                        .short('o')
                        .required(false)
                        .help("The JV Organization Name")
                )
                .arg(
                    Arg::new("prune")
                        .long("prune")
                        .short('p')
                        .action(clap::ArgAction::SetTrue)
                        .help("Prunes projects that no longer exist on Github")
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .short('a')
                        .action(clap::ArgAction::SetTrue)
                        .help("Include repositories that already exist locally")
                )
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .short('d')
                        .action(clap::ArgAction::SetTrue)
                        .help("Don't actually pull or prune anything")
                )
            )
            .subcommand(
                Command::new("push")
                    .about("Push repos for one or many organizations")
                    .arg(
                        Arg::new("organization-name")
                            .long("org")
                            .short('o')
                            .required(false)
                            .help("The JV Organization Name")
                    )
                    .arg(
                        Arg::new("all")
                            .long("all")
                            .short('a')
                            .action(clap::ArgAction::SetTrue)
                            .help("Include repositories that already contain a .git repo")
                    )
                    .arg(
                        Arg::new("dry-run")
                            .long("dry-run")
                            .short('d')
                            .action(clap::ArgAction::SetTrue)
                            .help("Don't actually push anything")
                    )
            )
            .subcommand(
                Command::new("delete")
                    .hide(true)
                    .about("Delete repos for one or more repositories")
                    .arg(
                        Arg::new("dry-run")
                            .long("dry-run")
                            .short('d')
                            .action(clap::ArgAction::SetTrue)
                            .help("Don't actually delete anything")
                    )
            )
        )
        .subcommand(Command::new("tilt")
            .about("Tilt Utilities")
            .subcommand(
                Command::new("generate")
                    .visible_alias("gen")
                    .about("Generates a Tilt configuration for an entire organization")
            )
        )
        .subcommand(Command::new("sso")
            .about("Configure access to kubernetes clusters via SSO")
            .subcommand(Command::new("aws")
                .about("Only configure SSO for AWS")
            )
            .subcommand(Command::new("azure")
                .about("Only configure SSO for Azure")
            )
            .subcommand(Command::new("auth0")
                .about("Only configure SSO for Auth0")
            )
        )
        .subcommand(Command::new("login")
            .about("Login to p6m services")
            .arg(
                Arg::new("organization-name")
                    .long("org")
                    .required(false)
                    .action(clap::ArgAction::Set)
                    .help("The JV Organization Name"),
            )
            .arg(
                Arg::new("refresh")
                    .long("refresh")
                    .short('r')
                    .action(clap::ArgAction::SetTrue)
                    .help("Refresh access tokens")
            )
        )
        .subcommand(Command::new("whoami")
            .about("Display information about the currently logged in user")
            .arg(
                Arg::new("output")
                            .long("output")
                            .short('o')
                            .help("Output format")
                            .value_parser(value_parser!(whoami::Output))
                            .default_value("default")
                            .required(false),
            )
            .arg(
                Arg::new("organization-name")
                    .long("org")
                    .required(false)
                    .action(clap::ArgAction::Set)
                    .help("The JV Organization Name")
            )
            .arg(
                Arg::new("authn-app-id")
                    .long("auth")
                    .required(false)
                    .action(clap::ArgAction::Set)
                    .help("Use an application ID which contains metadata for the authentication flow (meta.p6m.dev/authn-provider)")
            )
        )
        .arg(
            Arg::new("verbosity")
                .help("Increases logging verbosity level")
                .long("verbose")
                .short('v')
                .action(clap::ArgAction::Count)
                .global(true),
        )
        .arg(
            Arg::new("development")
                .long("dev")
                .action(clap::ArgAction::SetTrue)
                .hide(true)
                .help("Use the development environment.")
                .global(true),
            )
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum Environment {
    Dev,
    Staging,
    Prod,
}

impl Environment {}

#[derive(Debug, Clone)]
pub struct P6mEnvironment {
    pub config_dir: Utf8PathBuf,
    pub kube_dir: Utf8PathBuf,
    pub auth_dir: Utf8PathBuf,

    // Auth0
    pub auth_n: AuthN,
}

impl P6mEnvironment {
    pub fn init(matches: &ArgMatches) -> Result<Self, anyhow::Error> {
        let dev = matches.get_one::<bool>("development").cloned().unwrap();

        let home_dir = dirs::home_dir()
            .map(Utf8PathBuf::from_path_buf)
            .expect("Valid Home Directory Path")
            .expect("Utf8 Home Directory");

        let config_dir = match dev {
            true => home_dir.join(".p6m-dev"),
            false => home_dir.join(".p6m"),
        };

        let auth_n = AuthN {
            client_id: Some("j4jEhWwe2od1eacxuocy0sfmbf7V4H8V".into()),
            discovery_uri: Some("https://auth.p6m.run/.well-known/openid-configuration".into()),
            params: Some(
                vec![("audience".into(), "https://api.p6m.run/v1/".into())]
                    .into_iter()
                    .collect(),
            ),
            apps_uri: Some("https://auth.p6m.dev/api".into()),
            scopes: None,
            token_preference: Some(AuthToken::Id),
        };

        let environment = match dev {
            true => {
                println!("Using development environment");
                let mut auth_n = auth_n.clone();
                auth_n.apps_uri =
                    Some("https://9b6hcz5ny6.execute-api.us-east-2.amazonaws.com/api".into());
                auth_n.scopes = Some(vec!["urn:auth:dev:true".into()]);
                Self {
                    config_dir: config_dir.clone(),
                    kube_dir: home_dir.join(".kube"),
                    auth_dir: config_dir.join("auth"),
                    auth_n,
                }
            }
            false => Self {
                config_dir: config_dir.clone(),
                kube_dir: home_dir.join(".kube"),
                auth_dir: config_dir.join("auth"),
                auth_n,
            },
        };

        // Ensure this directory exist on behalf of all consumers
        create_dir_all(environment.config_dir())?;

        Ok(environment)
    }

    pub fn config_dir(&self) -> &Utf8Path {
        self.config_dir.as_path()
    }

    pub fn kube_dir(&self) -> &Utf8Path {
        self.kube_dir.as_path()
    }
}
