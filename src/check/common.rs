use clap::builder::PossibleValue;
use clap::ValueEnum;
use std::io::{BufRead, Lines};
use std::process::Command;
use strum_macros::EnumIter;

pub const CHECK_PREFIX: &str = "🔍";
pub const CHECK_SUCCESS: &str = "🟢";
pub const CHECK_ERROR: &str = "🔴";
// pub const CHECK_WARN: &str = "🟡";
pub const DOCS_PREFIX: &str = "https://developer.p6m.dev/docs/workstation";

pub fn print_see_also(path: &str) {
    println!("\n\t   See: {DOCS_PREFIX}/{path}");
}

pub fn print_success_lines(lines: Lines<&[u8]>, all_lines: bool) {
    lines
        .filter_map(|line| line.ok())
        .enumerate()
        .for_each(|(index, line)| {
            if index == 0 || all_lines {
                println!("\t{CHECK_SUCCESS} {line}");
            } else {
                println!("\t   {line}");
            }
        });
}

pub fn perform_check(
    check_name: &str,
    command: &mut Command,
    doc_path: &str,
) -> anyhow::Result<()> {
    println!("\n{CHECK_PREFIX} Checking {check_name}");

    match command.output() {
        Ok(output) => {
            if output.status.success() {
                print_success_lines(output.stdout.lines(), false);
            } else {
                println!("\t{CHECK_ERROR} {check_name} was found, but returned an unexpected Status Code: {}",  output.status.code().unwrap());
                print_see_also(doc_path);
            }
        }
        Err(_error) => {
            println!("\t{CHECK_ERROR} {check_name} is required, but was not found on the PATH");
            print_see_also(doc_path);
        }
    }

    Ok(())
}

#[derive(Clone, Copy, EnumIter)]
pub enum Ecosystem {
    Core,
    DotNet,
    Java,
    JavaScript,
    Python,
    Kubernetes,
}

impl ValueEnum for Ecosystem {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Ecosystem::Core,
            Ecosystem::DotNet,
            Ecosystem::JavaScript,
            Ecosystem::Java,
            Ecosystem::Python,
            Ecosystem::Kubernetes,
        ]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue> {
        Some(match self {
            Ecosystem::Core => PossibleValue::new("core"),
            Ecosystem::DotNet => PossibleValue::new("dotnet"),
            Ecosystem::JavaScript => PossibleValue::new("javascript"),
            Ecosystem::Java => PossibleValue::new("java"),
            Ecosystem::Python => PossibleValue::new("python"),
            Ecosystem::Kubernetes => PossibleValue::new("kubernetes"),
        })
    }
}
