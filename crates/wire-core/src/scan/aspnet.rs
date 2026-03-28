use super::types::DiscoveredEndpoint;
use std::path::Path;

/// Scan an ASP.NET project for HTTP endpoints.
/// Detects controller-based APIs and minimal APIs.
///
/// This is a placeholder — full implementation in a subsequent task.
pub fn scan_aspnet(_project_dir: &Path) -> (Vec<DiscoveredEndpoint>, usize) {
    (Vec::new(), 0)
}
