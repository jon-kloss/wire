use super::types::DiscoveredEndpoint;
use std::path::Path;

/// Scan a Node/Express project for HTTP endpoints.
/// Detects router.get/post/etc and app.get/post/etc patterns.
///
/// This is a placeholder — full implementation in a subsequent task.
pub fn scan_express(_project_dir: &Path) -> (Vec<DiscoveredEndpoint>, usize) {
    (Vec::new(), 0)
}
