use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{Context, Error};
use kube::config::{
    self, AuthInfo, Cluster, ExecConfig, Kubeconfig, NamedAuthInfo, NamedCluster, NamedContext,
    Preferences,
};
use log::{debug, info, warn};

use crate::{
    auth::{TokenRepository, TryReason},
    auth0,
    cli::P6mEnvironment,
    App, AuthToken,
};

pub async fn configure_auth0(
    environment: &P6mEnvironment,
    organization: Option<&String>,
) -> Result<(), Error> {
    let mut token_repository = TokenRepository::new(&environment.auth_n, &environment.auth_dir)?;

    if let Some(organization) = organization {
        token_repository.with_organization(organization)?;
    }

    token_repository
        .try_refresh(&TryReason::SsoCommand)
        .await
        .context("Please re-run `p6m login`")?;

    let id_token = token_repository
        .clone()
        .read_token(AuthToken::Id)
        .context("unable to read ID token")?;

    let email = token_repository
        .read_claims(AuthToken::Id)
        .context("unable to read claims")?
        .context("missing claims on the ID Token")?
        .email
        .context("missing email")?;

    let client = auth0::Client::new().with_token(id_token);

    let apps = client.apps().await.context("Unable to fetch apps")?;

    let kube_apps = apps.contain_scope("login:kubernetes");

    for app in kube_apps.clone() {
        let (kubeconfig, name) = generate_kubeconfig(&app, &email)
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

async fn generate_kubeconfig(app: &App, email: &String) -> Result<(Kubeconfig, String), Error> {
    let cluster_name = format!("p6m-{}", app.machine_name().replace("-auth0", ""));
    let url = app.url();
    let org = app.org().context("missing org")?;
    let ca = app.ca().context("Missing certificate authority")?;

    debug!(
        "found kube app: {:?}, url: {:?}, ca: {:?}",
        cluster_name, url, ca
    );

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
            certificate_authority_data: Some(ca.clone()),
            ..Default::default()
        }),
    }];

    let mut command: Vec<String> = vec![
        "p6m".into(),
        "whoami".into(),
        "--org".into(),
        org.clone().into(),
        "--output".into(),
        "k8s-auth".into(),
    ];

    let env: Vec<HashMap<String, String>> = vec![];

    let user_name = match app.auth_n {
        Some(_) => {
            // Seed the command with the app's client_id
            // - the client_id will be used to fetch meta.p6m.dev/authn-provider during whoami commands
            command.push("--auth".into());
            command.push(app.client_id.clone());

            // Leaving this here in case we need to add environment variables to the exec command
            // env.push(
            //     vec![
            //         ("name".to_string(), "SOME_ENV_VAR".to_string()),
            //         ("value".to_string(), "some value".to_string()),
            //     ]
            //     .into_iter()
            //     .collect(),
            // );

            format!("{} ({})", email, cluster_name)
        }
        None => format!("{} ({})", email, org),
    };

    kubeconfig.auth_infos = vec![NamedAuthInfo {
        name: user_name.clone(),
        auth_info: Some(AuthInfo {
            exec: Some(ExecConfig {
                api_version: Some("client.authentication.k8s.io/v1beta1".to_string()),
                command: command.first().cloned(),
                args: Some(command.iter().skip(1).cloned().collect()),
                interactive_mode: Some(config::ExecInteractiveMode::Always),
                env: match env.len() {
                    0 => None,
                    _ => Some(env),
                },
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

    kubeconfig.current_context = Some(cluster_name.clone());

    Ok((kubeconfig, cluster_name))
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
