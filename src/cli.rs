use crate::version;
use crate::{models::artifact, whoami};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{value_parser, Arg, ArgMatches, Command};
use clap_complete::Shell;
use std::fs::create_dir_all;
use crate::check::Ecosystem;

pub fn command() -> Command {
    clap::command!()
        .name("") // this string is prepended to -V and --version, resulting in invalid json
        .author("P6m Dev")
        .version(version::current_version())
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
        .subcommand(Command::new("check")
            .about("Check Development Environment")
            .arg(
                Arg::new("ecosystem")
                    .value_parser(value_parser!(Ecosystem))
                    .required(false)
                    .action(clap::ArgAction::Append)
                    .help("Ecosystem to check")
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
        )
        .subcommand(Command::new("login")
            .about("Login to p6m services")
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

#[derive(Debug)]
pub struct P6mEnvironment {
    pub config_dir: Utf8PathBuf,

    // Auth0
    pub domain: String,
    pub client_id: String,
    pub audience: String,
}

impl P6mEnvironment {
    pub fn init(matches: &ArgMatches) -> Result<Self, anyhow::Error> {
        let dev = matches.get_one::<bool>("development").cloned().unwrap();

        let home_dir = dirs::home_dir()
            .map(Utf8PathBuf::from_path_buf)
            .expect("Valid Home Directory Path")
            .expect("Utf8 Home Directory");

        let environment = match dev {
            true => {
                println!("Using development environment");
                Self {
                    config_dir: home_dir.join(".p6m-dev"),
                    domain: "p6m-dev.us.auth0.com".to_owned(),
                    client_id: "DkAzPi8iJITkDWKAoSjPON9jq6RSyCL9".to_owned(),
                    audience: "https://api-dev.p6m.dev/v1/".to_owned(),
                }
            }
            false => Self {
                config_dir: home_dir.join(".p6m"),
                domain: "auth.p6m.run".to_owned(),
                client_id: "j4jEhWwe2od1eacxuocy0sfmbf7V4H8V".to_owned(),
                audience: "https://api.p6m.run/v1/".to_owned(),
            },
        };

        // Ensure this directory exist on behalf of all consumers
        create_dir_all(environment.config_dir())?;

        Ok(environment)
    }

    pub fn config_dir(&self) -> &Utf8Path {
        self.config_dir.as_path()
    }
}
