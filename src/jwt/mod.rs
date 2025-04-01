use anyhow::{Error, Result};
use chrono::Duration;
use clap::ArgMatches;
use jsonwebtokens::{encode, Algorithm, AlgorithmID};
use serde_json::json;

use crate::cli::P6mEnvironment;

pub async fn execute(_: P6mEnvironment, matches: &ArgMatches) -> Result<()> {
    match matches.subcommand() {
        Some(("insecure", args)) => generate_jwt(args),
        Some((command, _)) => Err(Error::msg(format!(
            "Unimplemented sso command: '{}'",
            command
        ))),
        None => Ok(()),
    }?;

    Ok(())
}

pub fn generate_jwt(args: &ArgMatches) -> Result<()> {
    let alg = Algorithm::new_hmac(AlgorithmID::HS256, "insecure")?;
    let expires_days = args
        .get_one::<u32>("expire-days")
        .expect("Required by clap");
    let exp = chrono::Utc::now() + Duration::days((*expires_days) as i64);
    let header = json!({
        "alg": alg.name(),
        "typ": "JWT"
    });
    let claims = json!({
        "iss": "http://example.com",
        "sub": "1234567890",
        "exp": exp.timestamp(),
        "name": "John Doe",
        "admin": true,
        "scope": "products:read products:write orders:read",
    });
    let token = encode(&header, &claims, &alg)?;
    print!("{token}");
    Ok(())
}
