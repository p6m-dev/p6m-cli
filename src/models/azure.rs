use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AzureConfig {
    pub cloud_name: String,
    pub home_tenant_id: String,
    pub id: String,
    pub is_default: bool,
    // pub managedByTenants: Vec<Tenant>,
    pub name: String,
    pub state: AzureAccountState,
    pub tenant_id: String,
    // pub user: AzureUser,
}

impl Display for AzureConfig {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "AzureConfig: {}({})", self.name, self.id)
    }
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone, Copy)]
pub enum AzureAccountState {
    Enabled,
    Disabled,
}

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct AzureAccessToken {
    pub accessToken: String,
    pub expiresOn: String, // Dateformat - "2024-02-09 10:50:47.000000"
    pub subscription: String,
    pub tenant: String,
    pub tokenType: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct AzureAksCluster {
    pub ClusterName: String,
    pub ResourceGroup: String,
}

impl Display for AzureAksCluster {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "AKS Cluster: {}({})",
            self.ClusterName, self.ResourceGroup
        )
    }
}
