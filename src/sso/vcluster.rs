use std::{
    collections::BTreeMap,
    convert::TryFrom,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Error};
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::ListParams,
    config::{KubeConfigOptions, Kubeconfig},
    Client, Config,
};
use log::info;

pub async fn update_vcluster_kubecfgs(options: &KubeConfigOptions) -> Result<(), Error> {
    let config = create_config(options)
        .await
        .context("could not create kube config")?;

    let client = create_client(&config)
        .await
        .context("unable to create kube client")?;

    let secret_api: kube::Api<Secret> = kube::Api::all(client.clone());

    for secret in secret_api
        .list(&ListParams::default().labels(
            "p6m.dev/component=kubeconfig,meta.p6m.dev/controller=organization-controller-vcluster",
        ))
        .await?
    {
        match update_kubeconfig(&secret).await {
            Ok(update_res) => info!("vcluster: update-kubectx: {}", update_res),
            Err(err) => log::warn!("vcluster: unable to update kubeconfig: {}", err),
        }
    }

    Ok(())
}

async fn create_config(options: &KubeConfigOptions) -> Result<Config, Error> {
    match Config::from_kubeconfig(options).await {
        Ok(config) => Ok(config),
        Err(err) => {
            log::warn!("vcluster: unable to create config: {}", err);
            Err(anyhow::anyhow!(err))
        }
    }
}

async fn create_client(config: &Config) -> Result<Client, Error> {
    kube::Client::try_from(config.clone()).context("could not create client")
}

async fn update_kubeconfig(secret: &Secret) -> Result<String, Error> {
    let path = dirs::home_dir()
        .map(|path| path.join(".kube").join("config"))
        .unwrap_or_else(|| PathBuf::from(".kube").join("config"));

    let kubeconfig = Kubeconfig::read_from(path.as_path()).unwrap_or(Kubeconfig::default());

    let config = String::from_utf8(
        secret
            .data
            .as_ref()
            .unwrap_or(&BTreeMap::new())
            .get("config")
            .context("missing config on secret")?
            .clone()
            .0
            .clone(),
    )
    .context("unable to convert config to string")?;

    let mut new_kubeconfig =
        Kubeconfig::from_yaml(&config).context("couldn't create kube config from secret config")?;

    let server_name =
        uniqueify_kubeconfig(&mut new_kubeconfig).context("couldn't uniqueify kubeconfig")?;

    let kubeconfig = kubeconfig
        .merge(new_kubeconfig)
        .context("unable to merge configs")?;

    save_kubeconfig(&kubeconfig, path.as_path())
        .await
        .context("unable to save kube config")?;

    Ok(format!(
        "Updated context {} in {}",
        server_name,
        path.to_string_lossy(),
    )
    .to_string())
}

fn uniqueify_kubeconfig(kubeconfig: &mut Kubeconfig) -> Result<String, Error> {
    let current_context = kubeconfig
        .current_context
        .clone()
        .context("missing current context")?;

    let context = kubeconfig
        .contexts
        .iter_mut()
        .find(|c| c.name == current_context)
        .context("unable to find current named context")?;

    let cluster = kubeconfig
        .clusters
        .iter_mut()
        .find(|c| {
            Some(c.name.clone())
                == context
                    .context
                    .as_ref()
                    .context("missing context")
                    .ok()
                    .map(|c| c.cluster.clone())
        })
        .context("unable to find current named cluster")?;

    let auth_info = kubeconfig
        .auth_infos
        .iter_mut()
        .find(|a| {
            Some(a.name.clone())
                == context
                    .context
                    .as_ref()
                    .context("missing context")
                    .ok()
                    .map(|c| c.user.clone())
        })
        .context("unable to find current named auth info")?;

    let server_name = cluster
        .cluster
        .as_ref()
        .and_then(|c| c.server.clone())
        .context("missing server name")?
        .replace("https://", "");

    // Change every identifier to be server_name for uniqueness
    context.name = server_name.clone();
    cluster.name = server_name.clone();
    auth_info.name = server_name.clone();

    context.context.as_mut().map(|c| {
        c.cluster = cluster.name.clone();
        c.user = auth_info.name.clone();
    });

    Ok(server_name)
}

async fn save_kubeconfig(kubeconfig: &Kubeconfig, path: &Path) -> Result<(), Error> {
    let yaml = serde_yaml::to_string(kubeconfig).context("unable to convert kubeconfig to yaml")?;
    fs::write(path, yaml).context("unable to write kubeconfig")?;

    Ok(())
}
