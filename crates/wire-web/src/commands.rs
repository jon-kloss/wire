use crate::error::{AppError, AppResult};
use crate::sandbox::resolve_in_sandbox;
use crate::state::{SessionState, SharedState};
use crate::types::*;
use axum::{Extension, Json};
use std::path::Path;
use std::sync::Arc;
use wire_core::collection::{
    create_collection, list_templates, load_collection, load_request, load_request_resolved,
    rename_collection, Environment, LoadedCollection, WireRequest,
};
use wire_core::history::{self, HistoryEntry};
use wire_core::http::execute;
use wire_core::scan;
use wire_core::variables::VariableScope;

type Session = Extension<Arc<SessionState>>;

fn build_collection_info(collection: &LoadedCollection, wire_dir: &Path) -> IpcCollectionInfo {
    IpcCollectionInfo {
        name: collection.metadata.name.clone(),
        version: collection.metadata.version,
        active_env: collection.metadata.active_env.clone(),
        default_templates: collection.metadata.effective_default_templates(),
        requests: collection
            .requests
            .iter()
            .map(|(p, r)| IpcRequestEntry {
                path: p.to_string_lossy().to_string(),
                name: r.name.clone(),
                method: r.method.clone(),
            })
            .collect(),
        environments: collection.environments.keys().cloned().collect(),
        templates: list_templates(wire_dir).unwrap_or_default(),
        source_dir: collection.metadata.source_dir.clone(),
    }
}

fn sandbox_path(session: &SessionState, requested: &str) -> AppResult<std::path::PathBuf> {
    resolve_in_sandbox(&session.sandbox, requested).map_err(AppError)
}

/// Immediate subdirectories of `dir`, or empty if it can't be read.
fn subdirs(dir: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

/// Read the collection's display name from its `wire.yaml`, if present.
fn collection_name(wire_dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(wire_dir.join("wire.yaml")).ok()?;
    let metadata: wire_core::collection::WireCollection = serde_yaml::from_str(&content).ok()?;
    Some(metadata.name)
}

/// Turn a directory name like `express-api` into a label like `Express Api`.
fn prettify(dir: &Path) -> String {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    name.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------

/// Web-only: list the seeded samples that replace the desktop folder picker.
/// Derived by scanning the seeded sandbox so the seeds remain the single source
/// of truth — adding or renaming a seed dir is reflected here automatically.
pub async fn list_samples(Extension(session): Session) -> Json<Vec<SampleInfo>> {
    let sandbox = &session.sandbox;
    let mut samples = Vec::new();

    // Collections: any subdirectory of collections/ that contains a `.wire` dir.
    let collections_dir = sandbox.join("collections");
    for dir in subdirs(&collections_dir) {
        let wire_dir = dir.join(".wire");
        if wire_dir.is_dir() {
            samples.push(SampleInfo {
                label: collection_name(&wire_dir).unwrap_or_else(|| prettify(&dir)),
                description: "A ready-made collection that talks to the bundled demo API."
                    .to_string(),
                path: wire_dir.to_string_lossy().to_string(),
                kind: "collection".to_string(),
            });
        }
    }

    // Projects: every subdirectory of projects/.
    for dir in subdirs(&sandbox.join("projects")) {
        samples.push(SampleInfo {
            label: format!("{} (sample source)", prettify(&dir)),
            description: "A sample codebase — scan it to generate a collection.".to_string(),
            path: dir.to_string_lossy().to_string(),
            kind: "project".to_string(),
        });
    }

    // Stable ordering so the gallery doesn't shuffle between requests.
    samples
        .sort_by(|a, b| (a.kind.clone(), a.label.clone()).cmp(&(b.kind.clone(), b.label.clone())));

    // The sandbox workspace is always offered as a target for new collections.
    samples.push(SampleInfo {
        label: "Sandbox workspace".to_string(),
        description: "Your private, throwaway workspace for new collections.".to_string(),
        path: collections_dir.to_string_lossy().to_string(),
        kind: "location".to_string(),
    });

    Json(samples)
}

pub async fn open_collection(
    Extension(session): Session,
    Json(args): Json<OpenCollectionArgs>,
) -> AppResult<Json<IpcCollectionInfo>> {
    let path = sandbox_path(&session, &args.wire_dir)?;
    if !path.is_dir() {
        return Err(AppError(format!("Not a directory: {}", args.wire_dir)));
    }
    let collection = load_collection(&path).map_err(|e| e.to_string())?;
    let info = build_collection_info(&collection, &path);

    let mut inner = session.inner.lock().await;
    inner.collection_path = Some(path);
    inner.collection = Some(collection);
    Ok(Json(info))
}

pub async fn send_request(
    Extension(session): Session,
    axum::extract::State(state): axum::extract::State<SharedState>,
    Json(args): Json<SendRequestArgs>,
) -> AppResult<Json<IpcResponse>> {
    let file = sandbox_path(&session, &args.file)?;
    let inner = session.inner.lock().await;
    let request = if let Some(ref wire_dir) = inner.collection_path {
        load_request_resolved(&file, wire_dir).map_err(|e| e.to_string())?
    } else {
        load_request(&file).map_err(|e| e.to_string())?
    };

    let mut scope = VariableScope::new();
    if let Some(ref collection) = inner.collection {
        let active_env = args
            .env
            .clone()
            .or_else(|| collection.metadata.active_env.clone());
        if let Some(env_key) = active_env {
            if let Some(environment) = collection.environments.get(&env_key) {
                scope.push_layer(environment.variables.clone());
            }
        }
    }
    let history_path = history::resolve_history_path(inner.collection_path.as_deref());
    drop(inner);

    // Egress is enforced inside the HTTP client (see HttpClient::restricted_to).
    let response = execute(&state.http_client, &request, &scope)
        .await
        .map_err(|e| e.to_string())?;
    let ipc = IpcResponse::from(response);
    record_history(&history_path, &request, &ipc);
    Ok(Json(ipc))
}

pub async fn send_raw_request(
    Extension(session): Session,
    axum::extract::State(state): axum::extract::State<SharedState>,
    Json(args): Json<SendRawArgs>,
) -> AppResult<Json<IpcResponse>> {
    let inner = session.inner.lock().await;
    let request = if let Some(ref wire_dir) = inner.collection_path {
        let defaults = inner
            .collection
            .as_ref()
            .map(|c| c.metadata.effective_default_templates())
            .unwrap_or_default();
        let global_dirs: Vec<_> = wire_core::collection::global_templates_dir()
            .into_iter()
            .collect();
        wire_core::collection::resolve_with_defaults(
            args.request,
            wire_dir,
            &defaults,
            &global_dirs,
        )
        .map_err(|e| e.to_string())?
    } else {
        args.request
    };

    let mut scope = VariableScope::new();
    if let Some(ref collection) = inner.collection {
        let active_env = args
            .env
            .clone()
            .or_else(|| collection.metadata.active_env.clone());
        if let Some(env_key) = active_env {
            if let Some(environment) = collection.environments.get(&env_key) {
                scope.push_layer(environment.variables.clone());
            }
        }
    }
    let history_path = history::resolve_history_path(inner.collection_path.as_deref());
    drop(inner);

    // Egress is enforced inside the HTTP client (see HttpClient::restricted_to).
    let response = execute(&state.http_client, &request, &scope)
        .await
        .map_err(|e| e.to_string())?;
    let ipc = IpcResponse::from(response);
    record_history(&history_path, &request, &ipc);
    Ok(Json(ipc))
}

fn record_history(history_path: &Path, request: &WireRequest, ipc: &IpcResponse) {
    if let Err(e) = history::save_entry(
        history_path,
        &HistoryEntry {
            timestamp: chrono::Utc::now(),
            name: request.name.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            status: ipc.status,
            elapsed_ms: ipc.elapsed_ms,
        },
    ) {
        eprintln!("warning: failed to save history: {e}");
    }
}

pub async fn list_environments(Extension(session): Session) -> Json<Vec<String>> {
    let inner = session.inner.lock().await;
    let envs = inner
        .collection
        .as_ref()
        .map(|c| c.environments.keys().cloned().collect())
        .unwrap_or_default();
    Json(envs)
}

pub async fn list_history(
    Extension(session): Session,
    Json(args): Json<ListHistoryArgs>,
) -> AppResult<Json<Vec<HistoryEntry>>> {
    let inner = session.inner.lock().await;
    let history_path = history::resolve_history_path(inner.collection_path.as_deref());
    drop(inner);
    let entries = history::load_history(&history_path, args.limit.unwrap_or(50) as usize)
        .map_err(|e| e.to_string())?;
    Ok(Json(entries))
}

pub async fn clear_history(Extension(session): Session) -> AppResult<Json<()>> {
    let inner = session.inner.lock().await;
    let history_path = history::resolve_history_path(inner.collection_path.as_deref());
    drop(inner);
    history::clear_history(&history_path).map_err(|e| e.to_string())?;
    Ok(Json(()))
}

pub async fn create_collection_cmd(
    Extension(session): Session,
    Json(args): Json<CreateCollectionArgs>,
) -> AppResult<Json<IpcCollectionInfo>> {
    let parent = sandbox_path(&session, &args.parent_dir)?;
    let collection = create_collection(&parent, &args.name).map_err(|e| e.to_string())?;
    let wire_dir = parent.join(".wire");
    let info = build_collection_info(&collection, &wire_dir);

    let mut inner = session.inner.lock().await;
    inner.collection_path = Some(wire_dir);
    inner.collection = Some(collection);
    Ok(Json(info))
}

pub async fn rename_collection_cmd(
    Extension(session): Session,
    Json(args): Json<RenameCollectionArgs>,
) -> AppResult<Json<IpcCollectionInfo>> {
    let path = sandbox_path(&session, &args.wire_dir)?;
    let collection = rename_collection(&path, &args.new_name).map_err(|e| e.to_string())?;
    let info = build_collection_info(&collection, &path);

    let mut inner = session.inner.lock().await;
    inner.collection_path = Some(path);
    inner.collection = Some(collection);
    Ok(Json(info))
}

pub async fn scan_codebase(
    Extension(session): Session,
    Json(args): Json<ScanArgs>,
) -> AppResult<Json<IpcScanResult>> {
    let project_path = sandbox_path(&session, &args.project_dir)?;
    let output_path = sandbox_path(&session, &args.output_dir)?;

    let (scan_result, collection) =
        scan::scan_and_create_collection(&project_path, &output_path).map_err(|e| e.to_string())?;
    let framework = format!("{:?}", scan_result.framework);

    let (collection_info, wire_dir) = match collection {
        Some(col) => {
            let wire_dir = output_path.join(".wire");
            let info = build_collection_info(&col, &wire_dir);
            (Some(info), Some(wire_dir.to_string_lossy().to_string()))
        }
        None => (None, None),
    };

    Ok(Json(IpcScanResult {
        framework,
        endpoints_found: scan_result.endpoints.len(),
        files_scanned: scan_result.files_scanned,
        collection: collection_info,
        wire_dir,
    }))
}

pub async fn get_environment(
    Extension(session): Session,
    Json(args): Json<GetEnvArgs>,
) -> AppResult<Json<std::collections::HashMap<String, String>>> {
    let wire_dir = sandbox_path(&session, &args.wire_dir)?;
    let env_path = wire_dir
        .join("envs")
        .join(format!("{}.yaml", args.env_name));
    if !env_path.exists() {
        return Err(AppError(format!(
            "Environment file not found: {}",
            env_path.display()
        )));
    }
    let content = std::fs::read_to_string(&env_path).map_err(|e| e.to_string())?;
    let env: Environment = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
    Ok(Json(env.variables))
}

pub async fn save_environment(
    Extension(session): Session,
    Json(args): Json<SaveEnvArgs>,
) -> AppResult<Json<()>> {
    let wire_dir = sandbox_path(&session, &args.wire_dir)?;
    let env_path = wire_dir
        .join("envs")
        .join(format!("{}.yaml", args.env_name));

    let name = if env_path.exists() {
        let content = std::fs::read_to_string(&env_path).map_err(|e| e.to_string())?;
        let existing: Environment = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
        existing.name
    } else {
        if let Some(parent) = env_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        args.env_name.clone()
    };

    let env = Environment {
        name,
        variables: args.variables,
    };
    let yaml = serde_yaml::to_string(&env).map_err(|e| e.to_string())?;
    std::fs::write(&env_path, yaml).map_err(|e| e.to_string())?;
    Ok(Json(()))
}

pub async fn read_request(
    Extension(session): Session,
    Json(args): Json<ReadRequestArgs>,
) -> AppResult<Json<WireRequest>> {
    let file = sandbox_path(&session, &args.file)?;
    let inner = session.inner.lock().await;
    let request = if let Some(ref wire_dir) = inner.collection_path {
        let defaults = inner
            .collection
            .as_ref()
            .map(|c| c.metadata.effective_default_templates())
            .unwrap_or_default();
        let request = load_request(&file).map_err(|e| e.to_string())?;
        let global_dirs: Vec<_> = wire_core::collection::global_templates_dir()
            .into_iter()
            .collect();
        wire_core::collection::resolve_with_defaults(request, wire_dir, &defaults, &global_dirs)
            .map_err(|e| e.to_string())?
    } else {
        load_request(&file).map_err(|e| e.to_string())?
    };
    Ok(Json(request))
}

pub async fn list_templates_cmd(
    Extension(session): Session,
    Json(args): Json<ListTemplatesArgs>,
) -> AppResult<Json<Vec<String>>> {
    let wire_dir = sandbox_path(&session, &args.wire_dir)?;
    let templates = list_templates(&wire_dir).map_err(|e| e.to_string())?;
    Ok(Json(templates))
}

pub async fn read_template(
    Extension(session): Session,
    Json(args): Json<NameArgs>,
) -> AppResult<Json<WireRequest>> {
    let inner = session.inner.lock().await;
    let wire_dir = inner
        .collection_path
        .clone()
        .ok_or_else(|| AppError("No collection open".to_string()))?;
    drop(inner);
    let template = wire_core::collection::template::load_template(&args.name, &wire_dir)
        .map_err(|e| e.to_string())?;
    Ok(Json(template))
}

pub async fn save_request(
    Extension(session): Session,
    Json(args): Json<SaveRequestArgs>,
) -> AppResult<Json<()>> {
    let file_path = sandbox_path(&session, &args.path)?;
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directories: {e}"))?;
    }
    let yaml = serde_yaml::to_string(&args.request).map_err(|e| e.to_string())?;
    std::fs::write(&file_path, yaml).map_err(|e| format!("Failed to write file: {e}"))?;
    Ok(Json(()))
}

pub async fn delete_template(
    Extension(session): Session,
    Json(args): Json<NameArgs>,
) -> AppResult<Json<()>> {
    let name = args.name;
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AppError(format!("Invalid template name: {name}")));
    }
    let mut inner = session.inner.lock().await;
    let wire_dir = inner
        .collection_path
        .clone()
        .ok_or_else(|| AppError("No collection open".to_string()))?;

    let file_path = wire_dir.join("templates").join(format!("{name}.wire.yaml"));
    if !file_path.exists() {
        return Err(AppError(format!("Template not found: {name}")));
    }
    std::fs::remove_file(&file_path).map_err(|e| format!("Failed to delete template: {e}"))?;

    if let Some(ref mut col) = inner.collection {
        col.metadata.default_templates.retain(|t| t != &name);
    }
    drop(inner);

    let metadata_path = wire_dir.join("wire.yaml");
    if metadata_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&metadata_path) {
            if let Ok(mut metadata) =
                serde_yaml::from_str::<wire_core::collection::WireCollection>(&content)
            {
                metadata.default_templates.retain(|t| t != &name);
                if let Ok(yaml) = serde_yaml::to_string(&metadata) {
                    let _ = std::fs::write(&metadata_path, yaml);
                }
            }
        }
    }
    Ok(Json(()))
}

pub async fn save_template(
    Extension(session): Session,
    Json(args): Json<SaveTemplateArgs>,
) -> AppResult<Json<()>> {
    let name = args.name;
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AppError(format!(
            "Invalid template name: {name} (must not contain path separators or '..')"
        )));
    }
    let inner = session.inner.lock().await;
    let wire_dir = inner
        .collection_path
        .clone()
        .ok_or_else(|| AppError("No collection open".to_string()))?;
    drop(inner);

    let templates_dir = wire_dir.join("templates");
    std::fs::create_dir_all(&templates_dir)
        .map_err(|e| format!("Failed to create templates directory: {e}"))?;
    let file_path = templates_dir.join(format!("{name}.wire.yaml"));
    let yaml = serde_yaml::to_string(&args.request).map_err(|e| e.to_string())?;
    std::fs::write(&file_path, yaml).map_err(|e| format!("Failed to write template: {e}"))?;
    Ok(Json(()))
}

pub async fn toggle_default_template(
    Extension(session): Session,
    Json(args): Json<ToggleDefaultArgs>,
) -> AppResult<Json<Vec<String>>> {
    let wire_dir = sandbox_path(&session, &args.wire_dir)?;
    let metadata_path = wire_dir.join("wire.yaml");
    if !metadata_path.exists() {
        return Err(AppError("No wire.yaml found".to_string()));
    }
    let content = std::fs::read_to_string(&metadata_path).map_err(|e| e.to_string())?;
    let mut metadata: wire_core::collection::WireCollection =
        serde_yaml::from_str(&content).map_err(|e| e.to_string())?;

    if let Some(pos) = metadata
        .default_templates
        .iter()
        .position(|t| t == &args.template)
    {
        metadata.default_templates.remove(pos);
    } else {
        metadata.default_templates.push(args.template);
    }
    metadata.default_template = None;

    let result = metadata.default_templates.clone();
    let yaml = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    std::fs::write(&metadata_path, yaml).map_err(|e| e.to_string())?;

    let mut inner = session.inner.lock().await;
    if let Some(ref mut col) = inner.collection {
        col.metadata.default_templates = result.clone();
        col.metadata.default_template = None;
    }
    Ok(Json(result))
}

pub async fn check_drift(
    Extension(session): Session,
) -> AppResult<Json<wire_core::drift::DriftReport>> {
    let inner = session.inner.lock().await;
    let collection = inner
        .collection
        .as_ref()
        .ok_or_else(|| AppError("No collection open".to_string()))?;
    let source_dir = collection.metadata.source_dir.clone().ok_or_else(|| {
        AppError("Collection has no source directory (not generated from codebase)".to_string())
    })?;
    let requests = collection.requests.clone();
    drop(inner);

    let scan_result =
        wire_core::scan::scan_project(Path::new(&source_dir)).map_err(|e| e.to_string())?;
    Ok(Json(wire_core::drift::compare(
        &scan_result.endpoints,
        &requests,
    )))
}

pub async fn fix_drift(
    Extension(session): Session,
) -> AppResult<Json<wire_core::drift::DriftReport>> {
    let (requests, wire_dir, source_dir) = {
        let inner = session.inner.lock().await;
        let collection = inner
            .collection
            .as_ref()
            .ok_or_else(|| AppError("No collection open".to_string()))?;
        let source_dir = collection
            .metadata
            .source_dir
            .clone()
            .ok_or_else(|| AppError("Collection has no source directory".to_string()))?;
        let wire_dir = inner
            .collection_path
            .clone()
            .ok_or_else(|| AppError("No collection path".to_string()))?;
        (collection.requests.clone(), wire_dir, source_dir)
    };

    let scan_result =
        wire_core::scan::scan_project(Path::new(&source_dir)).map_err(|e| e.to_string())?;
    let report = wire_core::drift::compare(&scan_result.endpoints, &requests);
    let requests_dir = wire_dir.join("requests");
    let _ = std::fs::create_dir_all(&requests_dir);

    for item in &report.items {
        match item.category {
            wire_core::drift::DriftCategory::New => {
                if let Some(ep) = scan_result
                    .endpoints
                    .iter()
                    .find(|ep| ep.method == item.method && ep.route == item.route)
                {
                    let slug = item
                        .name
                        .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
                        .to_lowercase();
                    let group_dir = requests_dir.join(&ep.group);
                    let _ = std::fs::create_dir_all(&group_dir);
                    let mut file_path = group_dir.join(format!("{slug}.wire.yaml"));
                    if file_path.exists() {
                        file_path = group_dir
                            .join(format!("{}-{slug}.wire.yaml", item.method.to_lowercase()));
                    }
                    if !file_path.exists() {
                        let request = wire_core::scan::endpoint_to_request(ep);
                        if let Ok(yaml) = serde_yaml::to_string(&request) {
                            let _ = std::fs::write(&file_path, yaml);
                        }
                    }
                }
            }
            wire_core::drift::DriftCategory::Changed => {
                if let Some(ref req_path) = item.request_path {
                    if let Some(ep) = scan_result
                        .endpoints
                        .iter()
                        .find(|ep| ep.method == item.method && ep.route == item.route)
                    {
                        let request = wire_core::scan::endpoint_to_request(ep);
                        if let Ok(yaml) = serde_yaml::to_string(&request) {
                            let _ = std::fs::write(req_path, yaml);
                        }
                    }
                }
            }
            wire_core::drift::DriftCategory::Stale => {
                if let Some(ref req_path) = item.request_path {
                    let _ = std::fs::remove_file(req_path);
                }
            }
        }
    }

    let reloaded = wire_core::collection::load_collection(&wire_dir).map_err(|e| e.to_string())?;
    let fresh_requests = reloaded.requests.clone();
    {
        let mut inner = session.inner.lock().await;
        inner.collection = Some(reloaded);
    }
    Ok(Json(wire_core::drift::compare(
        &scan_result.endpoints,
        &fresh_requests,
    )))
}

pub async fn run_chain(
    Extension(session): Session,
    axum::extract::State(state): axum::extract::State<SharedState>,
    Json(args): Json<RunChainArgs>,
) -> AppResult<Json<wire_core::chain::ChainResult>> {
    let file = sandbox_path(&session, &args.file)?;
    let request = load_request(&file).map_err(|e| e.to_string())?;
    if request.chain.is_empty() {
        return Err(AppError(format!("No chain section found in {}", args.file)));
    }

    let inner = session.inner.lock().await;
    let wire_dir = inner
        .collection_path
        .clone()
        .ok_or_else(|| AppError("No collection open".to_string()))?;

    let mut scope = VariableScope::new();
    let mut resolved_env_name: Option<String> = None;
    if let Some(ref collection) = inner.collection {
        let active_env = args
            .env
            .clone()
            .or_else(|| collection.metadata.active_env.clone());
        if let Some(env_key) = active_env {
            if let Some(environment) = collection.environments.get(&env_key) {
                scope.push_layer(environment.variables.clone());
                resolved_env_name = Some(env_key.clone());
            }
        }
    }
    drop(inner);

    let result = wire_core::chain::execute_chain(
        &request.chain,
        &wire_dir,
        &scope,
        &state.http_client,
        resolved_env_name.as_deref(),
    )
    .await;
    Ok(Json(result))
}

pub async fn evaluate_tests(
    Json(args): Json<EvaluateTestsArgs>,
) -> AppResult<Json<Vec<wire_core::test::TestResult>>> {
    let wire_response: wire_core::http::WireResponse = args.response.into();
    Ok(Json(wire_core::test::evaluate_assertions(
        &args.assertions,
        &wire_response,
    )))
}
