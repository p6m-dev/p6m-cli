use crate::models::azure::{self, AzureAccessToken, AzureAksCluster, AzureConfig};
use anyhow::Error;
use log::{error, info, warn};
use std::process::Command;

pub async fn configure_azure() -> Result<(), Error> {
    let azure_configs = find_azure_accounts().unwrap_or(vec![]);
    if azure_configs.is_empty() {
        warn!("No Azure accounts found, make sure that you have run \n\n\taz login\nand have access to at least one Azure account.");
        return Ok(());
    }
    for azure_config in azure_configs {
        if azure_config.state == azure::AzureAccountState::Disabled {
            continue;
        }
        match find_azure_access_token(azure_config.clone()) {
            Ok(_) => {}
            Err(err) => {
                error!(
                    "Skipping {}, because failed to get access token. Error: {}",
                    &azure_config.name, err
                );
                continue;
            }
        };
        info!("list-clusters: {}", &azure_config.name);
        let aks_clusters = match get_aks_clusters(azure_config.clone()) {
            Ok(clusters) => clusters,
            Err(err) => {
                error!(
                    "Skipping {}, because failed to get AKS clusters. Error: {}",
                    &azure_config.name, err
                );
                continue;
            }
        };
        for cluster in aks_clusters {
            info!("aks: update-kubectx: {}", &cluster.ClusterName);
            match update_kubeconfig(azure_config.clone(), cluster.clone()) {
                Ok(_) => {}
                Err(err) => {
                    error!(
                        "Failed to update kubeconfig for AKS cluster {}. Error: {}",
                        &cluster.ClusterName, err
                    );
                }
            };
        }
    }

    Ok(())
}

fn find_azure_accounts() -> Result<Vec<AzureConfig>, Error> {
    let mut cmd: Command = Command::new("az");
    cmd.args(&["account", "list", "--all"]);

    log::debug!("executing `{:?}`", cmd);
    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => return Err(Error::msg(format!("unable to run 'az aks list': {}", err))),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if let Some(exist_status) = output.status.code() {
        if exist_status != 0 {
            return Err(Error::msg(format!(
                "unable to list Azure accounts: {}",
                stderr
            )));
        }
    } else {
        return Err(Error::msg("Command terminated by signal"));
    }

    let config: Vec<AzureConfig> = match serde_json::from_str(&stdout) {
        Ok(config) => config,
        Err(_) => {
            warn!("invalid json: {}", &stdout);
            return Err(Error::msg("invalid json"));
        }
    };
    Ok(config)
}

fn find_azure_access_token(azure_config: AzureConfig) -> Result<(), Error> {
    let mut cmd: Command = Command::new("az");
    cmd.args(&[
        "account",
        "get-access-token",
        "--subscription",
        &azure_config.id,
    ]);

    log::debug!("executing `{:?}`", cmd);
    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => {
            return Err(Error::msg(format!(
                "unable to run 'az get-access-token': {}",
                err
            )))
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if let Some(exist_status) = output.status.code() {
        if exist_status != 0 {
            return Err(Error::msg(format!(
                "unable to get access token for {}: {}",
                &azure_config.name, stderr
            )));
        }
    } else {
        return Err(Error::msg("Command terminated by signal"));
    }

    let _token: AzureAccessToken = match serde_json::from_str(&stdout) {
        Ok(token) => token,
        Err(_) => {
            warn!("invalid json: {}", &stdout);
            return Err(Error::msg("invalid json"));
        }
    };
    Ok(())
}

fn get_aks_clusters(azure_config: AzureConfig) -> Result<Vec<AzureAksCluster>, Error> {
    let mut cmd: Command = Command::new("az");
    cmd.args(&[
        "aks",
        "list",
        "--query",
        "[].{ClusterName:name, ResourceGroup:resourceGroup}",
        "--subscription",
        &azure_config.id,
    ]);

    log::debug!("executing `{:?}`", cmd);
    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => return Err(Error::msg(format!("unable to run 'az aks list': {}", err))),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if let Some(exist_status) = output.status.code() {
        if exist_status != 0 {
            return Err(Error::msg(format!(
                "unable to list clusters for {}: {}",
                &azure_config.name, stderr
            )));
        }
    } else {
        return Err(Error::msg("Command terminated by signal"));
    }

    let clusters: Vec<AzureAksCluster> = match serde_json::from_str(&stdout) {
        Ok(clusters) => clusters,
        Err(_) => {
            warn!("invalid json: {}", &stdout);
            return Err(Error::msg("invalid json"));
        }
    };
    Ok(clusters)
}

fn update_kubeconfig(azure_config: AzureConfig, cluster: AzureAksCluster) -> Result<(), Error> {
    let mut cmd: Command = Command::new("az");
    cmd.args(&[
        "aks",
        "get-credentials",
        "--name",
        &cluster.ClusterName,
        "--resource-group",
        &cluster.ResourceGroup,
        "--context",
        &cluster.ClusterName,
        "--subscription",
        &azure_config.id,
        "--overwrite-existing",
    ]);

    log::debug!("executing `{:?}`", cmd);
    let output = match cmd.output() {
        Ok(o) => o,
        Err(err) => {
            return Err(Error::msg(format!(
                "unable to run 'az aks get-credentials': {}",
                err
            )))
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr);

    match output.status.code() {
        Some(0) => Ok(()),
        Some(_) => Err(Error::msg(format!(
            "unable to update kubeconfig for {}: {}",
            &cluster.ClusterName, stderr
        ))),
        None => Err(Error::msg("Command terminated by signal")),
    }
}
