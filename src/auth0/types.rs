use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use urlencoding::decode;

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
pub struct App {
    pub name: String,
    pub org: Option<String>,
    pub client_id: String,
    pub url: String,
    pub origins: Vec<String>,
    pub scopes: Vec<String>,
}

impl App {
    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }

    pub fn org(&self) -> Option<String> {
        self.org.clone()
    }

    pub fn certificate_authority(&self) -> Result<String> {
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
