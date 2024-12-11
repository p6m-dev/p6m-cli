use clap::ArgMatches;
use crate::check::common::*;

const ARTIFACTORY_TOKEN_KEY: &str = "ARTIFACTORY_IDENTITY_TOKEN";
const ARTIFACTORY_USER_KEY: &str = "ARTIFACTORY_USERNAME";

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    check_artifact_management_tokens(args)?;
    Ok(())
}

fn check_artifact_management_tokens(_args: &ArgMatches) -> anyhow::Result<()> {
    println!("\n{CHECK_PREFIX} Checking Artifact Management Tokens");
    if let (Ok(identity), Ok(token)) =
        (std::env::var(ARTIFACTORY_USER_KEY), std::env::var(ARTIFACTORY_TOKEN_KEY)) {
        if identity.is_empty() || token.is_empty() {
            print_missing_token_error();
        }
        println!("\t{CHECK_SUCCESS} Artifactory Tokens Found");
    } else {
        print_missing_token_error();
    }
    Ok(())
}

fn print_missing_token_error() {
    println!("\t{CHECK_ERROR} {ARTIFACTORY_USER_KEY} and/or {ARTIFACTORY_TOKEN_KEY} environment variables have not been set correctly.");
    print_see_also("artifacts");
}