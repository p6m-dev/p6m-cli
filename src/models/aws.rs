use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct AwsConfig {
    pub accessToken: String,
    pub expiresAt: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AwsAccountInfo {
    pub account_id: String,
    pub account_slug: String,
}

#[derive(Serialize, Deserialize)]
pub struct AwsAccountRoleInfo {
    pub account_id: String,
    pub account_slug: String,
    pub role_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct AwsEksListClustersResponse {
    pub clusters: Vec<String>,
}
