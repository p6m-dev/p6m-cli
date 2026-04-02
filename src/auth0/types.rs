use std::collections::BTreeMap;

use anyhow::{Context, Result};
use log::trace;
use serde::{Deserialize, Serialize};
use urlencoding::decode;

// AKS AAD client ID — only supports device code flow, not interactive browser auth.
// https://azure.github.io/kubelogin/concepts/aks.html#azure-kubernetes-service-aad-server
const AKS_AAD_CLIENT_ID: &str = "80faf920-1908-4b52-b5ef-a8e7bedfc67a";

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
    #[strum(to_string = "CLIENT_ID")]
    #[serde(rename = "client_id")]
    ClientId,
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
    pub apps_uri: Option<String>,
    pub scopes: Option<Vec<String>>,
}

impl AuthN {
    /// Returns true if this auth provider uses interactive browser login (PKCE)
    /// instead of device code flow. Determined by a localhost redirect_uri in params.
    pub fn is_interactive(&self) -> bool {
        if self.client_id.as_deref() == Some(AKS_AAD_CLIENT_ID) {
            return false;
        }

        self.redirect_uri()
            .map(|uri| uri.starts_with("http://localhost"))
            .unwrap_or(false)
    }

    /// Returns the redirect_uri from params, if configured.
    pub fn redirect_uri(&self) -> Option<&String> {
        self.params.as_ref().and_then(|p| p.get("redirect_uri"))
    }

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

    /// Returns scopes for this auth provider.
    /// Checks explicit scopes first, then falls back to the `scopes` param
    /// from meta.p6m.dev/authn-provider extraParams.
    pub fn additional_scopes(&self) -> Vec<String> {
        self.scopes
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.params
                    .as_ref()
                    .and_then(|p| p.get("scopes"))
                    .map(|s| s.split_whitespace().map(String::from).collect())
            })
            .unwrap_or_default()
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
