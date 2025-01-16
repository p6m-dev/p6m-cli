use std::{fs, path::PathBuf};

use anyhow::{Context, Error};
use kube::config::{
    self, AuthInfo, Cluster, ExecConfig, Kubeconfig, NamedAuthInfo, NamedCluster, NamedContext,
    Preferences,
};
use log::{debug, info, warn};

use crate::{
    auth::{AuthToken, TokenRepository},
    auth0,
    cli::P6mEnvironment,
};

const BASE_URL: &str = "https://auth0.us-east-2.aws.prd.p6m.run/api";

pub async fn configure_auth0(
    environment: &P6mEnvironment,
    organization: Option<&String>,
) -> Result<(), Error> {
    let mut token_repository = TokenRepository::new(environment)?;

    if let Some(organization) = organization {
        token_repository.with_organization(organization)?;
    }

    let id_token = token_repository
        .clone()
        .read_token(AuthToken::Id)
        .context("unable to read access token")?;

    let email = token_repository
        .read_claims(AuthToken::Id)
        .context("unable to read claims")?
        .context("missing claims on the ID Token")?
        .email
        .context("missing email")?;

    let client = auth0::Client::new()
        .with_base_url(&BASE_URL.to_string())
        .with_token(id_token);

    let apps = client.apps().await.context("Unable to fetch apps")?;

    let kube_apps = apps.contain_scope("login:kubernetes");

    for app in kube_apps.clone() {
        let name = app.name();
        let url = app.url();
        let org = match app.org() {
            Some(org) => org,
            _ => {
                warn!(
                    "Skipping: Kubernetes App {:?} is missing organization.",
                    name
                );
                continue;
            }
        };
        let user_name = format!("{} ({})", email, org);
        let certificate_authority_data = match app.certificate_authority() {
            Ok(ca) => ca,
            Err(_) => {
                warn!(
                    "Skipping: Kubernetes App {:?} is missing certificate authority.",
                    name
                );
                continue;
            }
        };

        debug!(
            "found kube app: {:?}, url: {:?}, ca: {:?}",
            name, url, certificate_authority_data
        );

        let kubeconfig =
            generate_kubeconfig(&name, &user_name, &url, &org, &certificate_authority_data)
                .await
                .context("unable to generate kubeconfig")?;

        match merge_kubeconfig(kubeconfig, &name).await {
            Ok(update_res) => {
                info!("auth0: update-kubectx: {}", update_res);
            }
            Err(err) => {
                warn!("auth0: unable to update kubeconfig: {}", err);
            }
        };
    }

    Ok(())
}

async fn generate_kubeconfig(
    cluster_name: &String,
    user_name: &String,
    url: &String,
    org: &String,
    ca_data: &String,
) -> Result<Kubeconfig, Error> {
    let mut kubeconfig = Kubeconfig::default();
    kubeconfig.api_version = Some("v1".to_string());
    kubeconfig.kind = Some("Config".to_string());
    kubeconfig.preferences = Some(Preferences {
        colors: None,
        extensions: None,
    });

    kubeconfig.clusters = vec![NamedCluster {
        name: url.clone(),
        cluster: Some(Cluster {
            server: Some(url.clone()),
            certificate_authority_data: Some(ca_data.clone()),
            ..Default::default()
        }),
    }];

    kubeconfig.auth_infos = vec![NamedAuthInfo {
        name: user_name.clone(),
        auth_info: Some(AuthInfo {
            exec: Some(ExecConfig {
                api_version: Some("client.authentication.k8s.io/v1beta1".to_string()),
                command: Some("p6m".into()),
                args: Some(vec![
                    "whoami".into(),
                    "--org".into(),
                    org.into(),
                    "--output".into(),
                    "k8s-auth".into(),
                ]),
                interactive_mode: None,
                env: None,
                drop_env: None,
            }),
            ..Default::default()
        }),
    }];

    kubeconfig.contexts = vec![NamedContext {
        name: cluster_name.clone(),
        context: Some(config::Context {
            cluster: url.clone(),
            user: user_name.clone(),
            ..Default::default()
        }),
    }];

    // kubeconfig.current_context = Some(name.clone());

    Ok(kubeconfig)
}

async fn merge_kubeconfig(kubeconfig: Kubeconfig, name: &String) -> Result<String, Error> {
    let path = dirs::home_dir()
        .map(|path| path.join(".kube").join("config"))
        .unwrap_or_else(|| PathBuf::from(".kube").join("config"));

    let existing = Kubeconfig::read_from(path.clone().as_path()).unwrap_or(Kubeconfig::default());

    let kubeconfig = kubeconfig
        .merge(existing)
        .context("unable to merge configs")?;

    let yaml =
        serde_yaml::to_string(&kubeconfig).context("unable to convert kubeconfig to yaml")?;

    fs::write(path.clone(), yaml).context("unable to write kubeconfig")?;

    Ok(format!("Updated context {} in {}", name, path.to_string_lossy(),).to_string())
}
