/// A discovered HTTP endpoint from source code analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredEndpoint {
    /// Group name for organizing into folders (controller name, router file, or route prefix)
    pub group: String,
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
    /// Discovered body field names and their type hints (e.g., [("name", "string"), ("age", "int")])
    pub body_fields: Vec<(String, String)>,
    /// Response type name if detected (e.g., "TourDto", "List<TourDto>")
    pub response_type: Option<String>,
    /// Discovered response field names and their type hints
    pub response_fields: Vec<(String, String)>,
}

/// Detected project framework.
#[derive(Debug, Clone, PartialEq)]
pub enum Framework {
    AspNet,
    Express,
    NextJs,
    FastApi,
    SpringBoot,
    Unknown,
}

/// Result of scanning a project directory.
#[derive(Debug)]
pub struct ScanResult {
    pub framework: Framework,
    pub endpoints: Vec<DiscoveredEndpoint>,
    pub files_scanned: usize,
}
