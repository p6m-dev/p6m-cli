use clap::{ArgMatches, ValueEnum};
use strum::IntoEnumIterator;

mod common;
mod check_java;
mod check_javascript;
mod check_python;
mod check_dotnet;
mod check_scm;
mod check_docker;
mod check_kubernetes;
mod check_artifact_management;
mod check_archetect;

pub use common::Ecosystem;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {

    if let Some(ecosystems) = args.get_many::<Ecosystem>("ecosystem") {
        for ecosystem in ecosystems {
            check_ecosystem(ecosystem, args)?;
        }
    } else {
        for ecosystem in Ecosystem::iter() {
            check_ecosystem(&ecosystem, args)?;
        }
    }

    Ok(())
}

fn check_ecosystem(ecosystem: &Ecosystem, args: &ArgMatches) -> anyhow::Result<()> {
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
    }

    Ok(())
}