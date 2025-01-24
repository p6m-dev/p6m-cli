use clap::{ArgMatches, ValueEnum};
use strum::IntoEnumIterator;

mod check_archetect;
mod check_artifact_management;
mod check_docker;
mod check_dotnet;
mod check_java;
mod check_javascript;
mod check_kubernetes;
mod check_python;
mod check_scm;
mod common;
mod check_self;

pub use common::Ecosystem;

pub async fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    if let Some(ecosystems) = args.get_many::<Ecosystem>("ecosystem") {
        for ecosystem in ecosystems {
            check_ecosystem(ecosystem, args).await?;
        }
    } else {
        for ecosystem in Ecosystem::iter() {
            check_ecosystem(&ecosystem, args).await?;
        }
    }

    Ok(())
}

pub async fn execute_interactive(args: &ArgMatches) -> anyhow::Result<()> {
    let ecosystems = Ecosystem::value_variants().iter().map(|ecosystem| ecosystem.to_string())
        .collect::<Vec<String>>();
    let prompt = inquire::MultiSelect::new("Ecosystems:", ecosystems);
    match prompt.prompt_skippable() {
        Ok(Some(ecosystems)) => {
            let ecosystems = ecosystems.iter().map(|ecosystem| Ecosystem::from_str(ecosystem, true).expect("Cannot fail"))
                .collect::<Vec<Ecosystem>>();
            for ecosystem in ecosystems {
                check_ecosystem(&ecosystem, args).await?
            }
        }
        Err(_) => {}
        Ok(None) => {}
    }

    Ok(())

}

async fn check_ecosystem(ecosystem: &Ecosystem, args: &ArgMatches) -> anyhow::Result<()> {
    match ecosystem {
        Ecosystem::Core => {
            check_archetect::execute(args)?;
            check_scm::execute(args)?;
            check_docker::execute(args)?;
            check_artifact_management::execute(args)?;
        }
        Ecosystem::DotNet => {
            check_dotnet::execute(args)?;
        }
        Ecosystem::Java => {
            check_java::execute(args)?;
        }
        Ecosystem::JavaScript => {
            check_javascript::execute(args)?;
        }
        Ecosystem::Kubernetes => {
            check_kubernetes::execute(args)?;
        }
        Ecosystem::Python => {
            check_python::execute(args)?;
        }
        Ecosystem::P6mCli => {
            check_self::execute(args).await?;
        }
    }

    Ok(())
}
