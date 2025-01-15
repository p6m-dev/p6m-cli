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

        let kubeconfig = generate_kubeconfig(&name, &url, &org, &certificate_authority_data)
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
    name: &String,
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
        name: org.clone(),
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
        name: name.clone(),
        context: Some(config::Context {
            cluster: url.clone(),
            user: org.clone(),
            ..Default::default()
        }),
    }];

    // kubeconfig.current_context = Some(name.clone());

    Ok(kubeconfig)
}

// pub async fn update_vcluster_kubecfgs(options: &KubeConfigOptions) -> Result<(), Error> {
//     let config = create_config(options)
//         .await
//         .context("could not create kube config")?;

//     let client = create_client(&config)
//         .await
//         .context("unable to create kube client")?;

//     let secret_api: kube::Api<Secret> = kube::Api::all(client.clone());

//     for secret in secret_api
//         .list(&ListParams::default().labels(
//             "p6m.dev/component=kubeconfig,meta.p6m.dev/controller=organization-controller-vcluster",
//         ))
//         .await?
//     {
//         match update_kubeconfig(&secret).await {
//             Ok(update_res) => info!("vcluster: update-kubectx: {}", update_res),
//             Err(err) => log::warn!("vcluster: unable to update kubeconfig: {}", err),
//         }
//     }

//     Ok(())
// }

// async fn create_config(options: &KubeConfigOptions) -> Result<Config, Error> {
//     match Config::from_kubeconfig(options).await {
//         Ok(config) => Ok(config),
//         Err(err) => {
//             log::warn!("vcluster: unable to create config: {}", err);
//             Err(anyhow::anyhow!(err))
//         }
//     }
// }

// async fn create_client(config: &Config) -> Result<Client, Error> {
//     kube::Client::try_from(config.clone()).context("could not create client")
// }

// async fn update_kubeconfig(secret: &Secret) -> Result<String, Error> {
//     let path = dirs::home_dir()
//         .map(|path| path.join(".kube").join("config"))
//         .unwrap_or_else(|| PathBuf::from(".kube").join("config"));

//     let kubeconfig = Kubeconfig::read_from(path.as_path()).unwrap_or(Kubeconfig::default());

//     let config = String::from_utf8(
//         secret
//             .data
//             .as_ref()
//             .unwrap_or(&BTreeMap::new())
//             .get("config")
//             .context("missing config on secret")?
//             .clone()
//             .0
//             .clone(),
//     )
//     .context("unable to convert config to string")?;

//     let mut new_kubeconfig =
//         Kubeconfig::from_yaml(&config).context("couldn't create kube config from secret config")?;

//     let server_name =
//         uniqueify_kubeconfig(&mut new_kubeconfig).context("couldn't uniqueify kubeconfig")?;

//     let kubeconfig = kubeconfig
//         .merge(new_kubeconfig)
//         .context("unable to merge configs")?;

//     save_kubeconfig(&kubeconfig, path.as_path())
//         .await
//         .context("unable to save kube config")?;

//     Ok(format!(
//         "Updated context {} in {}",
//         server_name,
//         path.to_string_lossy(),
//     )
//     .to_string())
// }

// fn uniqueify_kubeconfig(kubeconfig: &mut Kubeconfig) -> Result<String, Error> {
//     let current_context = kubeconfig
//         .current_context
//         .clone()
//         .context("missing current context")?;

//     let context = kubeconfig
//         .contexts
//         .iter_mut()
//         .find(|c| c.name == current_context)
//         .context("unable to find current named context")?;

//     let cluster = kubeconfig
//         .clusters
//         .iter_mut()
//         .find(|c| {
//             Some(c.name.clone())
//                 == context
//                     .context
//                     .as_ref()
//                     .context("missing context")
//                     .ok()
//                     .map(|c| c.cluster.clone())
//         })
//         .context("unable to find current named cluster")?;

//     let auth_info = kubeconfig
//         .auth_infos
//         .iter_mut()
//         .find(|a| {
//             Some(a.name.clone())
//                 == context
//                     .context
//                     .as_ref()
//                     .context("missing context")
//                     .ok()
//                     .map(|c| c.user.clone())
//         })
//         .context("unable to find current named auth info")?;

//     let server_name = cluster
//         .cluster
//         .as_ref()
//         .and_then(|c| c.server.clone())
//         .context("missing server name")?
//         .replace("https://", "");

//     // Change every identifier to be server_name for uniqueness
//     context.name = server_name.clone();
//     cluster.name = server_name.clone();
//     auth_info.name = server_name.clone();

//     context.context.as_mut().map(|c| {
//         c.cluster = cluster.name.clone();
//         c.user = auth_info.name.clone();
//     });

//     Ok(server_name)
// }

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
