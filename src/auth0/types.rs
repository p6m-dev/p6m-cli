use std::collections::BTreeMap;

use anyhow::{Context, Result};
use log::trace;
use serde::{Deserialize, Serialize};
use urlencoding::decode;

#[derive(Debug, Serialize, Deserialize, strum_macros::Display, Clone)]
pub enum AuthToken {
    #[strum(to_string = "ACCESS_TOKEN")]
    #[serde(rename = "access_token")]
    Access,
    #[strum(to_string = "ID_TOKEN")]
    #[serde(rename = "id_token")]
    Id,
    #[strum(to_string = "REFRESH_TOKEN")]
    #[serde(rename = "refresh_token")]
    Refresh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Apps(Vec<App>);

impl Apps {
    pub fn contain_scope(self, scope: &str) -> Self {
        Self(
            self.0
                .into_iter()
                .filter(|f| f.scopes.contains(&scope.to_string()))
                .collect(),
        )
    }
}

impl IntoIterator for Apps {
    type Item = App;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthN {
    pub client_id: Option<String>,
    pub discovery_uri: Option<String>,
    pub token_preference: Option<AuthToken>,
    pub params: Option<BTreeMap<String, String>>,
    pub acr_values: Option<Vec<String>>,
    pub apps_uri: Option<String>,
}

impl AuthN {
    pub fn apps_uri(&self) -> Option<String> {
        return self.apps_uri.clone();
    }

    pub fn login_form_data(&self, scope: &String) -> Result<BTreeMap<String, String>> {
        let mut form = BTreeMap::new();
        form.insert(
            "client_id".to_string(),
            self.client_id.clone().context("missing client_id")?,
        );
        if scope.trim().len() > 0 {
            form.insert("scope".to_string(), scope.clone());
        }
        if let Some(params) = self.params.clone() {
            form.extend(params);
        }
        trace!("login_form_data: {:?}", form);
        Ok(form)
    }

    pub fn device_code_form_data(&self, device_code: &String) -> Result<BTreeMap<String, String>> {
        let mut form = BTreeMap::new();
        form.insert(
            "client_id".to_string(),
            self.client_id.clone().context("missing client_id")?,
        );
        form.insert("code".to_string(), device_code.to_string());
        form.insert("device_code".to_string(), device_code.to_string());
        form.insert(
            "grant_type".to_string(),
            "urn:ietf:params:oauth:grant-type:device_code".to_string(),
        );
        if let Some(acr_values) = self.acr_values.clone() {
            form.insert("acr_values".to_string(), acr_values.join(" "));
        }
        trace!("device_code_form_data: {:?}", form);
        Ok(form)
    }

    pub fn refresh_form_data(&self, refresh_token: &String) -> Result<BTreeMap<String, String>> {
        let mut form = BTreeMap::new();
        form.insert(
            "client_id".to_string(),
            self.client_id.clone().context("missing client_id")?,
        );
        form.insert("refresh_token".to_string(), refresh_token.to_string());
        form.insert("grant_type".to_string(), "refresh_token".to_string());
        trace!("refresh_form_data: {:?}", form);
        Ok(form)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub name: String,
    pub org: Option<String>,
    pub client_id: String,
    pub url: String,
    pub origins: Vec<String>,
    pub scopes: Vec<String>,
    pub metadata: BTreeMap<String, String>,
    pub auth_n: Option<AuthN>,
}

impl App {
    pub fn display_name(&self) -> String {
        self.name.clone()
    }

    pub fn machine_name(&self) -> String {
        self.metadata
            .get("ClaimName")
            .map(|s| s.to_string())
            .unwrap_or(self.display_name())
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect()
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }

    pub fn org(&self) -> Option<String> {
        self.org.clone()
    }

    pub fn ca(&self) -> Result<String> {
        let certificate_authority = self
            .origins
            .iter()
            .find(|origin| origin.starts_with("https://meta.p6m.dev/certificate-authority"))
            .map(|origin| {
                url::Url::parse(origin)
                    .context("unalbe to parse url")
                    .map(|u| Some(u.fragment()?.to_string()))
                    .context("unable to extract fragment")
            })
            .context("unable to find certificate authority")?
            .context("missing certificate authority")?;

        Ok(
            decode(certificate_authority.context("missing ca")?.as_str())
                .context("unable to decode ca")?
                .to_string(),
        )
    }
}
