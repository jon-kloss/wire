mod executor;

pub use executor::execute;

use crate::error::WireError;
use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;

/// Wrapper around reqwest::Client providing Wire-specific HTTP functionality.
pub struct HttpClient {
    client: reqwest::Client,
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
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent(format!("Wire/{}", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client })
    }

    pub fn inner(&self) -> &reqwest::Client {
        &self.client
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new().expect("failed to create HTTP client")
    }
}
