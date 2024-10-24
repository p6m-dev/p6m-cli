#[cfg(target_os = "windows")]
use std::path::PathBuf;

use crate::models::{
    artifact::StorageProvider,
    git::{GithubLevel, Organization},
};
use anyhow::Error;
use base64::{engine, Engine};
use clap::ArgMatches;
use minijinja::render;
use tokio::fs;

macro_rules! read_env_var_only_if {
    ($active_storage:expr, $storage_provider:expr, $env_var_name:literal) => {
        if $active_storage == &$storage_provider {
            std::env::var($env_var_name).map_err(|_| {
                Error::msg($env_var_name.to_owned() + " environment variable must be set.")
            })?
        } else {
            "".to_owned()
        }
    };
}

macro_rules! new_file_with_content {
    ($dir: expr, $file_name: literal, $content: expr) => {
        if !$dir.exists() {
            fs::create_dir_all($dir.clone()).await?;
        }

        let mut file = $dir.clone();
        file.push($file_name);

        fs::write(file, $content).await?;
    };
}

pub async fn execute(matches: &ArgMatches) -> Result<(), Error> {
    let organization =
        GithubLevel::with_organization(matches.get_one::<String>("organization-name"))?
            .organization()
            .unwrap();
    let provider = matches
        .get_one::<StorageProvider>("provider")
        .cloned()
        .unwrap_or_default();
    set_context(&organization, &provider).await
}

async fn set_context(
    organization: &Organization,
    active_storage: &StorageProvider,
) -> Result<(), Error> {
    let organization_name = organization.name().to_owned();
    let artifactory_username = read_env_var_only_if!(
        active_storage,
        StorageProvider::Artifactory,
        "ARTIFACTORY_USERNAME"
    );
    let artifactory_identity_token = read_env_var_only_if!(
        active_storage,
        StorageProvider::Artifactory,
        "ARTIFACTORY_IDENTITY_TOKEN"
    );
    let cloudsmith_username = read_env_var_only_if!(
        active_storage,
        StorageProvider::Cloudsmith,
        "CLOUDSMITH_USERNAME"
    );
    let cloudsmith_api_key = read_env_var_only_if!(
        active_storage,
        StorageProvider::Cloudsmith,
        "CLOUDSMITH_API_KEY"
    );

    let home_dir = dirs::home_dir().ok_or(Error::msg("Unable to obtain home directory path"))?;

    // Maven

    let mut m2_dir = home_dir.to_path_buf();
    m2_dir.push(".m2");

    new_file_with_content!(
        m2_dir,
        "settings.xml",
        render!(
            include_str!("../resources/settings.xml"),
            organization_name,
            active_storage,
            artifactory_username,
            artifactory_identity_token,
            cloudsmith_username,
            cloudsmith_api_key,
        )
    );

    // NPM

    let registry_url = match active_storage {
        StorageProvider::Artifactory => format!(
            "p6m.jfrog.io/artifactory/api/npm/{}-npm/",
            organization_name
        ),
        StorageProvider::Cloudsmith => {
            format!("npm.cloudsmith.io/p6m-dev/{}/", organization_name)
        }
    };
    let platform_registry_url = match active_storage {
        StorageProvider::Artifactory => "p6m.jfrog.io/artifactory/api/npm/p6m-npm/",
        StorageProvider::Cloudsmith => "npm.cloudsmith.io/p6m-dev/p6m-run/",
    };
    let auth_config = match active_storage {
        StorageProvider::Artifactory => {
            let b64engine = engine::general_purpose::STANDARD;
            let basic_auth = b64engine.encode(
                format!("{}:{}", artifactory_username, artifactory_identity_token).as_bytes(),
            );
            format!("_auth={}", basic_auth)
        }
        StorageProvider::Cloudsmith => format!("_authToken={}", cloudsmith_api_key),
    };

    new_file_with_content!(
        home_dir,
        ".npmrc",
        render!(
            include_str!("../resources/npmrc"),
            registry_url,
            platform_registry_url,
            auth_config,
        )
    );

    // Python

    #[cfg(target_os = "windows")]
    let poetry_config_dir = {
        let mut config = PathBuf::from(
            std::env::var("APPDATA")
                .expect("No APPDATA environment variable. Are you sure you are on Windows?"),
        );
        config.push("pypoetry");
        config
    };
    #[cfg(target_os = "macos")]
    let poetry_config_dir = {
        let mut config = home_dir.to_path_buf();
        config.push("Library");
        config.push("Application Support");
        config.push("pypoetry");
        config
    };
    #[cfg(all(target_family = "unix", not(target_os = "macos")))]
    let poetry_config_dir = {
        let mut config = home_dir.to_path_buf();
        config.push(".config");
        config.push("pypoetry");
        config
    };

    let username = match active_storage {
        StorageProvider::Artifactory => artifactory_username.clone(),
        StorageProvider::Cloudsmith => cloudsmith_username.clone(),
    };

    let password = match active_storage {
        StorageProvider::Artifactory => artifactory_identity_token.clone(),
        StorageProvider::Cloudsmith => cloudsmith_api_key.clone(),
    };

    new_file_with_content!(
        poetry_config_dir,
        "auth.toml",
        render!(
            include_str!("../resources/poetry/auth.toml.j2"),
            organization_name => organization_name.replace('-', "_"),
            username,
            password,
        )
    );

    let alt_publishing_url = match active_storage {
        StorageProvider::Artifactory => format!(
            "https://p6m.jfrog.io/artifactory/api/pypi/{}-pypi/",
            organization_name
        ),
        StorageProvider::Cloudsmith => format!(
            "https://python.cloudsmith.io/p6m-dev/{}/",
            organization_name
        ),
    };

    new_file_with_content!(
        poetry_config_dir,
        "config.toml",
        render!(
            include_str!("../resources/poetry/config.toml.j2"),
            organization_name => organization_name.replace('-', "_"),
            alt_publishing_url,
        )
    );

    Ok(())
}
