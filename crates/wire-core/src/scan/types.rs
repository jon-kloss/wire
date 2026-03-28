/// A discovered HTTP endpoint from source code analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredEndpoint {
    /// HTTP method (GET, POST, PUT, PATCH, DELETE)
    pub method: String,
    /// Route pattern with Wire {{variable}} syntax (e.g., /api/users/{{id}})
    pub route: String,
    /// Human-readable name (e.g., "GetUsers", "CreateUser")
    pub name: String,
    /// Discovered header parameters (name, description)
    pub headers: Vec<(String, String)>,
    /// Discovered query parameters (name, description)
    pub query_params: Vec<(String, String)>,
    /// Body type name if detected (e.g., "CreateUserDto")
    pub body_type: Option<String>,
}

/// Detected project framework.
#[derive(Debug, Clone, PartialEq)]
pub enum Framework {
    AspNet,
    Express,
    Unknown,
}

/// Result of scanning a project directory.
#[derive(Debug)]
pub struct ScanResult {
    pub framework: Framework,
    pub endpoints: Vec<DiscoveredEndpoint>,
    pub files_scanned: usize,
}
