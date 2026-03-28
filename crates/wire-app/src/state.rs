use std::path::PathBuf;
use tokio::sync::Mutex;
use wire_core::collection::LoadedCollection;
use wire_core::http::HttpClient;

pub struct AppState {
    pub http_client: HttpClient,
    pub collection: Mutex<Option<LoadedCollection>>,
    pub collection_path: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            http_client: HttpClient::default(),
            collection: Mutex::new(None),
            collection_path: Mutex::new(None),
        }
    }
}
