use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use url::Url;

use super::types::Apps;

#[derive(Debug, Clone)]
pub struct Client {
    base_url: Option<String>,
    token: Option<String>,
    client: Option<reqwest::Client>,
}

impl Client {
    pub fn new() -> Self {
        Self {
            base_url: None,
            token: None,
            client: reqwest::Client::builder()
                .user_agent(format!("p6m-cli/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .ok(),
        }
    }

    pub fn with_base_url(mut self, base_url: &String) -> Self {
        self.base_url = Some(base_url.clone());
        self
    }

    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token.clone();
        self
    }

    async fn authorization(&self) -> Result<String> {
        Ok(format!(
            "Bearer {}",
            self.token.as_ref().context("Missing token")?
        ))
    }

    pub async fn apps(&self) -> Result<Apps> {
        Request::new(self)
            .with_authorization(&self.authorization().await?)
            .with_method(&reqwest::Method::GET)
            .with_endpoint("apps")?
            .send::<Apps>()
            .await?
            .context("Missing apps")
    }
}

#[derive(Debug, Clone)]
struct Request {
    client: Client,
    authorization: Option<String>,
    url: Option<String>,
    method: Option<reqwest::Method>,
    payload: Option<serde_json::Value>,
    allow_conflict: Option<bool>,
}

impl Request {
    pub fn new(client: &Client) -> Self {
        Self {
            client: client.clone(),
            authorization: None,
            url: None,
            method: None,
            payload: None,
            allow_conflict: None,
        }
    }

    pub fn with_authorization(mut self, authorization: &str) -> Self {
        self.authorization = Some(authorization.to_string());
        self
    }

    pub fn with_endpoint(mut self, endpoint: &str) -> Result<Self> {
        let base_url = self
            .client
            .base_url
            .as_ref()
            .context("Base URL not found in Client")?;

        let mut url =
            Url::parse(base_url.trim_end_matches("/")).context("Failed to parse domain")?;

        url.path_segments_mut()
            .ok()
            .context("Failed to get path segments")?
            .extend(endpoint.split("/"));

        self.url = Some(url.to_string());

        Ok(self)
    }

    #[allow(dead_code)]
    pub fn with_query(mut self, key: &str, value: &str) -> Result<Self> {
        let url = self.url.as_ref().context("URL not found")?;

        let mut url = Url::parse(url).context("Failed to parse URL")?;

        url.query_pairs_mut().append_pair(key, value);

        self.url = Some(url.to_string());

        Ok(self)
    }

    pub fn with_method(mut self, method: &reqwest::Method) -> Self {
        self.method = Some(method.clone());
        self
    }

    #[allow(dead_code)]
    pub fn with_payload(mut self, payload: &serde_json::Value) -> Self {
        self.payload = Some(payload.clone());

        self
    }

    #[allow(dead_code)]
    pub fn with_allow_conflict(mut self, allow_conflict: bool) -> Self {
        self.allow_conflict = Some(allow_conflict);
        self
    }

    async fn send<T>(&self) -> Result<Option<T>>
    where
        T: serde::de::DeserializeOwned + std::fmt::Debug,
    {
        let client = self.client.client.as_ref().context("Client not found")?;

        let method = self.method.as_ref().context("Method not found")?;

        let url = self.url.as_ref().context("URL not found")?;

        let request_builder = client.request(method.clone(), url);

        let request_builder = match self.authorization.as_ref() {
            Some(authorization) => request_builder.header("Authorization", authorization),
            None => request_builder,
        };

        let response = match self.payload.as_ref() {
            Some(payload) => request_builder
                .json(payload)
                .send()
                .await
                .context(format!("Failed to {} {:?} to {}", method, payload, url))?,
            None => request_builder
                .send()
                .await
                .context(format!("Failed to {} {}", method, url))?,
        };

        if let Err(error) = response.error_for_status_ref() {
            let status = error.status().context("Missing status")?;

            match (status, self.allow_conflict) {
                (reqwest::StatusCode::CONFLICT, Some(true)) => {
                    return Ok(None);
                }
                (reqwest::StatusCode::UNAUTHORIZED, _) => {
                    return Err(anyhow!("Please run `p6m login`"));
                }
                _ => {}
            }

            let body = response
                .text()
                .await
                .context("Failed to get error response text")?;
            match self.payload.as_ref() {
                Some(payload) => {
                    return Err(anyhow!(
                        "{} {} with payload `{:?}` responded with status {}: {:?}",
                        method,
                        url,
                        payload,
                        status,
                        body,
                    ))?
                }
                None => {
                    return Err(anyhow!(
                        "{} {}: {}: {}",
                        method,
                        url,
                        status,
                        match serde_json::from_str::<Value>(&body) {
                            Ok(json) => format!("{}", json),
                            Err(_) => format!("{body}"),
                        }
                    ))?
                }
            }
        }

        let text = response
            .text()
            .await
            .context("Failed to get response text")?;

        let response: T = serde_json::from_str::<T>(&text)
            .context(format!("Failed to parse response: {}", text))?;

        Ok(Some(response))
    }
}
