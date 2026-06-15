mod executor;

pub use executor::execute;

use crate::error::WireError;
use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;

/// Wrapper around reqwest::Client providing Wire-specific HTTP functionality.
pub struct HttpClient {
    client: reqwest::Client,
    /// Optional egress allowlist of (host, port) pairs. When `Some`, requests to
    /// any other host are refused before they leave the process. `None` (the
    /// default for the CLI and desktop app) allows all hosts.
    allowed_hosts: Option<Vec<(String, Option<u16>)>>,
}

/// The result of executing an HTTP request.
#[derive(Debug, Clone, Serialize)]
pub struct WireResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub elapsed: Duration,
    pub size_bytes: usize,
}

impl HttpClient {
    pub fn new() -> Result<Self, WireError> {
        Ok(Self {
            client: Self::build_client()?,
            allowed_hosts: None,
        })
    }

    /// Build a client whose outbound requests are restricted to the hosts of the
    /// given base URLs. Used by the web playground to confine requests to the
    /// bundled demo API. Because the check lives here, every execution path
    /// (single sends and chains alike) is covered and none can bypass it.
    pub fn restricted_to(base_urls: &[String]) -> Result<Self, WireError> {
        let mut allowed = Vec::new();
        for base in base_urls {
            let url = reqwest::Url::parse(base)
                .map_err(|e| WireError::Other(format!("Invalid allowlist URL '{base}': {e}")))?;
            let host = url
                .host_str()
                .ok_or_else(|| WireError::Other(format!("Allowlist URL '{base}' has no host")))?
                .to_string();
            allowed.push((host, url.port_or_known_default()));
        }
        Ok(Self {
            client: Self::build_client()?,
            allowed_hosts: Some(allowed),
        })
    }

    fn build_client() -> Result<reqwest::Client, WireError> {
        Ok(reqwest::Client::builder()
            .cookie_store(true)
            .user_agent(format!("Wire/{}", env!("CARGO_PKG_VERSION")))
            .build()?)
    }

    pub fn inner(&self) -> &reqwest::Client {
        &self.client
    }

    /// Refuse requests to hosts outside the egress allowlist. A no-op when no
    /// allowlist is configured.
    pub fn check_egress(&self, url: &str) -> Result<(), WireError> {
        let Some(ref allowed) = self.allowed_hosts else {
            return Ok(());
        };
        let parsed = reqwest::Url::parse(url)
            .map_err(|e| WireError::Other(format!("Invalid URL '{url}': {e}")))?;
        let host = parsed.host_str().unwrap_or_default();
        let port = parsed.port_or_known_default();
        if allowed.iter().any(|(h, p)| h == host && *p == port) {
            Ok(())
        } else {
            Err(WireError::Other(format!(
                "Request to '{host}' was blocked: this demo only permits requests to the bundled API."
            )))
        }
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new().expect("failed to create HTTP client")
    }
}
