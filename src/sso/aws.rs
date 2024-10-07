use crate::models::aws::{
    AwsAccountInfo, AwsAccountRoleInfo, AwsConfig, AwsEksListClustersResponse,
};
use anyhow::Error;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_eks::config::Region;
use chrono::{Duration, Utc};
use futures_util::StreamExt;
use log::{info, warn};
use minijinja::render;
use sha1::{Digest, Sha1};
use std::{
    env,
    fs::{self, File},
    io::Write,
    process::Command,
};

static SSO_PROFILE_NAME: &str = "ybor";

// Lower index is higher priority; Roles not in list are ranked below all others
// TODO: Remove AdministratorAccess once dev control plane role assignments are working
const AWS_ROLE_HIERARCHY: [&str; 4] =
    ["administrator", "AdministratorAccess", "owner", "developer"];

pub async fn configure_aws() -> Result<(), Error> {
    // Create the initial aws config file with the Ybor SSO session. This covers the use case where the
    // user is configuring this for the first time and there is no SSO config at all for downstream calls.
    let mut aws_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")
        .expect("Unable to get home directory");
    aws_dir.push(".aws");
    let aws_config_file_path = aws_dir.join("config");

    // Check to make sure AWS_* is not set
    // TODO this can probably be removed if the aws_config below is built manually.
    check_env_unset("AWS_PROFILE")?;
    check_env_unset("AWS_ACCESS_KEY_ID")?;
    check_env_unset("AWS_SECRET_ACCESS_KEY")?;
    check_env_unset("AWS_SESSION_TOKEN")?;

    let empty_aws_config = render!(include_str!("../../resources/aws_config"));
    create_or_replace_file(aws_config_file_path.clone().to_str(), &empty_aws_config)
        .expect("Unable to overwrite ~/.aws/config");

    let config = aws_config::from_env()
        .region(Region::new("us-east-2"))
        .load()
        .await;
    let sso_client = aws_sdk_sso::Client::new(&config);
    let page_size = 10;

    let access_token = find_aws_access_token(SSO_PROFILE_NAME)?;

    // Loop through every account to populate the AwsAccountInfo vector
    let account_vector = find_accounts(sso_client.clone(), access_token.clone(), page_size).await;

    // Loop through every account to populate the AwsAccountRoleInfo vector
    let mut account_role_vector: Vec<AwsAccountRoleInfo> = Vec::new();

    for account in account_vector.iter() {
        match find_account_role(
            sso_client.clone(),
            access_token.clone(),
            page_size,
            account.clone(),
        )
        .await
        {
            Some(role_name) => {
                info!("aws: sso: {} {}", account.account_slug, role_name);
                account_role_vector.push(AwsAccountRoleInfo {
                    account_id: account.account_id.clone(),
                    account_slug: account.account_slug.clone(),
                    role_name,
                });
            }
            None => {
                warn!("aws: sso: no roles found for {}", account.account_slug);
            }
        }
    }

    // Write to ~/.aws/config again, this time with all the JV profiles
    let content = render!(
        include_str!("../../resources/aws_config"),
        account_role_vector
    );
    create_or_replace_file(aws_config_file_path.clone().to_str(), &content)
        .expect("Unable to overwrite ~/.aws/config");

    // Find clusters and update kubeconfig for each JV
    for account in account_vector.iter() {
        let res = cmd_list_clusters(account.account_slug.clone());
        info!("aws: list-clusters: {}", account.account_slug.clone());
        match res {
            Ok(list_clusters_res) => {
                list_clusters_res.clusters.iter().for_each(|cluster| {
                    let update_res =
                        cmd_update_kubecfg(account.account_slug.clone(), cluster.to_string());
                    match update_res {
                        Ok(_) => info!("aws: update-kubectx: {}", cluster),
                        Err(err) => {
                            log::warn!("aws: unable to update kubeconfig': {}", err);
                        }
                    }
                });
            }
            Err(err) => warn!("Unable to list clusters: {}", err),
        }
    }
    Ok(())
}

// This manually finds the cached aws SSO access_token on the
// filesystem. It should be in a json file in ~/.aws/sso/cache
// where the filename is the SHA1 hash.
//
// Certain calls to the AWS SSO API require the token, even though
// it is stored in the local cache and could probably be pulled
// from there. There is a feature request for this to happen in
// V2 of the aws-cli.
//
// See https://github.com/aws/aws-cli/issues/5057 for details.
fn find_aws_access_token(sso_profile_name: &str) -> Result<String, Error> {
    // Find AWS SSO cache dir
    let mut aws_cache_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")
        .expect("Unable to get home directory");
    aws_cache_dir.push(".aws");
    aws_cache_dir.push("sso");
    aws_cache_dir.push("cache");

    // SHA1 hash of the profile
    let mut hasher = Sha1::new();
    hasher.update(sso_profile_name);
    let hasher_result = hasher.finalize();
    let filename = hex::encode(hasher_result) + ".json";

    // Parse the json and return the access token
    let file_path = aws_cache_dir.join(filename);
    match fs::read_to_string(file_path) {
        Ok(contents_json) => {
            let parsed_json: AwsConfig =
                serde_json::from_str(&contents_json).expect("file contents are not valid JSON");

            let now = Utc::now();
            let duration_until_timestamp = parsed_json.expiresAt - now;
            if duration_until_timestamp < Duration::zero() {
                return Err(Error::msg(format!("sso token expired at {}, try logging in?\n\n\taws sso login --sso-session ybor\n", parsed_json.expiresAt)));
            }

            // Return the accessToken
            Ok(parsed_json.accessToken)
        }
        Err(_) => Err(Error::msg(
            "unable to find AWS sso token, try logging in?\n\n\taws sso login --sso-session ybor\n",
        )),
    }
}

async fn find_accounts(
    sso_client: aws_sdk_sso::Client,
    access_token: String,
    page_size: i32,
) -> Vec<AwsAccountInfo> {
    let mut account_vector: Vec<AwsAccountInfo> = Vec::new();

    // Get all AWS accounts for the current SSO session
    let mut list_accounts = sso_client
        .list_accounts()
        .set_access_token(Some(access_token))
        .into_paginator()
        .page_size(page_size)
        .send();

    while let Some(item) = list_accounts.next().await {
        match item {
            Ok(val) => val.account_list.into_iter().for_each(|account_vec| {
                account_vec.into_iter().for_each(|account| {
                    let account_id = account.account_id.expect("empty account id");
                    let account_email = account.email_address.expect("empty account email");
                    let account_slug = email_to_org_slug(account_email);

                    account_vector.push(AwsAccountInfo {
                        account_id,
                        account_slug,
                    });
                })
            }),
            Err(err) => warn!("Unable to list accounts: {}", err.into_service_error()),
        }
    }
    account_vector
}

async fn find_account_role(
    sso_client: aws_sdk_sso::Client,
    access_token: String,
    page_size: i32,
    account: AwsAccountInfo,
) -> Option<String> {
    let mut list_roles = sso_client
        .list_account_roles()
        .set_access_token(Some(access_token.clone()))
        .set_account_id(Some(account.clone().account_id))
        .into_paginator()
        .page_size(page_size)
        .send();

    let mut selected_role: Option<(String, usize)> = None;
    while let Some(item) = list_roles.next().await {
        match item {
            Ok(val) => val.role_list.into_iter().for_each(|role_vec| {
                role_vec.into_iter().for_each(|role| {
                    let current_role_name = role.role_name.expect("empty role name");
                    let current_rank = AWS_ROLE_HIERARCHY
                        .iter()
                        .position(|&r| r == current_role_name)
                        .unwrap_or(usize::MAX);

                    if let Some((_, selected_rank)) = selected_role {
                        if current_rank < selected_rank {
                            selected_role = Some((current_role_name.clone(), current_rank));
                        }
                    } else {
                        selected_role = Some((current_role_name.clone(), current_rank));
                    }
                })
            }),
            Err(err) => warn!("Unable to list roles: {}", err.into_service_error()),
        }
    }
    match selected_role {
        Some((role_name, _)) => Some(role_name),
        None => None,
    }
}

// Create or replace a file with the specified content, and create the directory structure if it is missing
fn create_or_replace_file(filename: Option<&str>, content: &str) -> Result<(), Error> {
    if let Some(file_path) = filename {
        if let Some(parent_dir) = std::path::Path::new(file_path).parent() {
            fs::create_dir_all(parent_dir)?;
        }

        let mut file = File::create(file_path)?;
        file.write_all(content.as_bytes())?;
    } else {
        return Err(Error::msg("filename is not specified"));
    }

    Ok(())
}

// Takes an email for a JV (platform+aws-jv-name@ybor.ai) and converts it to a profile name
fn email_to_org_slug(email: String) -> String {
    let mut s = email.as_str();
    while let Some(rest) = s.strip_prefix("platform+aws-") {
        s = rest;
    }
    while let Some(rest) = s.strip_suffix("@ybor.ai") {
        s = rest;
    }
    return s.to_string();
}

fn cmd_list_clusters(profile: String) -> Result<AwsEksListClustersResponse, Error> {
    let mut cmd = Command::new("aws");
    cmd.args(&["eks", "list-clusters"]);
    cmd.env("AWS_PROFILE", profile.clone());

    log::debug!("executing `{:?}`", cmd);

    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => {
            return Err(Error::msg(format!(
                "unable to run 'aws eks list-clusters': {}",
                err
            )));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if let Some(exit_status) = output.status.code() {
        if exit_status != 0 {
            return Err(Error::msg(format!(
                "unable to list clusters for {}: {}",
                profile.clone(),
                stderr
            )));
        }
    } else {
        return Err(Error::msg("Command terminated by signal"));
    }

    let res = serde_json::from_str(&stdout);

    match res {
        Ok(json_res) => return Ok(json_res),
        Err(_) => {
            log::warn!("invalid json: {}", &stdout);
            return Err(Error::msg("invalid json"));
        }
    }
}

fn cmd_update_kubecfg(profile: String, cluster: String) -> Result<String, Error> {
    let mut cmd = Command::new("aws");
    cmd.args(&[
        "eks",
        "update-kubeconfig",
        "--name",
        cluster.as_str(),
        "--alias",
        cluster.clone().as_str(),
    ]);
    cmd.env("AWS_PROFILE", profile.clone());

    log::debug!("executing `{:?}`", cmd);

    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => {
            log::warn!(
                "unable to run 'aws eks update-kubeconfig --name {}': {}",
                profile.clone(),
                err
            );
            return Err(Error::msg("command error"));
        }
    };

    let out = output.stdout;

    // Attempt to convert the Vec<u8> into a String
    match String::from_utf8(out) {
        Ok(string) => return Ok(string),
        Err(e) => {
            log::warn!("unable to parse output: {}", e);
            return Err(Error::msg("parsing error"));
        }
    }
}

fn check_env_unset(env_var: &str) -> Result<(), Error> {
    match env::var(env_var) {
        Ok(_) => return Err(Error::msg(format!("{} must be unset for this to work correctly. Try this in your current terminal session:\n\n\tunset {}\n", env_var, env_var))),
        Err(_) => return Ok(()),
    }
}
