use crate::state::AppState;
use crate::types::{IpcCollectionInfo, IpcRequestEntry, IpcResponse, IpcScanResult};
use std::path::Path;
use tauri::State;
use wire_core::collection::{
    create_collection, list_templates, load_collection, load_request, load_request_resolved,
    rename_collection, Assertion, Environment, WireRequest,
};
use wire_core::history::{self, HistoryEntry};
use wire_core::http::execute;
use wire_core::scan;
use wire_core::variables::VariableScope;

#[tauri::command]
pub async fn open_collection(
    wire_dir: String,
    state: State<'_, AppState>,
) -> Result<IpcCollectionInfo, String> {
    let path = Path::new(&wire_dir);
    if !path.is_dir() {
        return Err(format!("Not a directory: {wire_dir}"));
    }

    let collection = load_collection(path).map_err(|e| e.to_string())?;

    let info = IpcCollectionInfo {
        name: collection.metadata.name.clone(),
        version: collection.metadata.version,
        active_env: collection.metadata.active_env.clone(),
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
    };

    *state.collection_path.lock().await = Some(path.to_path_buf());
    *state.collection.lock().await = Some(collection);

    Ok(info)
}

#[tauri::command]
pub async fn send_request(
    file: String,
    env: Option<String>,
    state: State<'_, AppState>,
) -> Result<IpcResponse, String> {
    let col_path = state.collection_path.lock().await;
    let request = if let Some(ref wire_dir) = *col_path {
        load_request_resolved(Path::new(&file), wire_dir).map_err(|e| e.to_string())?
    } else {
        load_request(Path::new(&file)).map_err(|e| e.to_string())?
    };
    drop(col_path);

    let mut scope = VariableScope::new();

    // Load environment variables if a collection is open
    let collection_guard = state.collection.lock().await;
    if let Some(ref collection) = *collection_guard {
        let active_env = env.or_else(|| collection.metadata.active_env.clone());
        if let Some(env_key) = active_env {
            if let Some(environment) = collection.environments.get(&env_key) {
                scope.push_layer(environment.variables.clone());
            }
        }
    }
    drop(collection_guard);

    let response = execute(&state.http_client, &request, &scope)
        .await
        .map_err(|e| e.to_string())?;

    // Fire-and-forget history recording
    let ipc_response = IpcResponse::from(response);
    let col_path = state.collection_path.lock().await;
    let history_path = history::resolve_history_path(col_path.as_deref());
    drop(col_path);
    if let Err(e) = history::save_entry(
        &history_path,
        &HistoryEntry {
            timestamp: chrono::Utc::now(),
            name: request.name.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            status: ipc_response.status,
            elapsed_ms: ipc_response.elapsed_ms,
        },
    ) {
        eprintln!("warning: failed to save history: {e}");
    }

    Ok(ipc_response)
}

#[tauri::command]
pub async fn list_environments(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let collection_guard = state.collection.lock().await;
    match *collection_guard {
        Some(ref collection) => Ok(collection.environments.keys().cloned().collect()),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn send_raw_request(
    request: WireRequest,
    env: Option<String>,
    state: State<'_, AppState>,
) -> Result<IpcResponse, String> {
    // Resolve template if extends is set
    let request = if request.extends.is_some() {
        let col_path = state.collection_path.lock().await;
        if let Some(ref wire_dir) = *col_path {
            wire_core::collection::resolve_template(request, wire_dir).map_err(|e| e.to_string())?
        } else {
            request
        }
    } else {
        request
    };

    let mut scope = VariableScope::new();

    let collection_guard = state.collection.lock().await;
    if let Some(ref collection) = *collection_guard {
        let active_env = env.or_else(|| collection.metadata.active_env.clone());
        if let Some(env_key) = active_env {
            if let Some(environment) = collection.environments.get(&env_key) {
                scope.push_layer(environment.variables.clone());
            }
        }
    }
    drop(collection_guard);

    let response = execute(&state.http_client, &request, &scope)
        .await
        .map_err(|e| e.to_string())?;

    // Fire-and-forget history recording
    let ipc_response = IpcResponse::from(response);
    let col_path = state.collection_path.lock().await;
    let history_path = history::resolve_history_path(col_path.as_deref());
    drop(col_path);
    if let Err(e) = history::save_entry(
        &history_path,
        &HistoryEntry {
            timestamp: chrono::Utc::now(),
            name: request.name.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            status: ipc_response.status,
            elapsed_ms: ipc_response.elapsed_ms,
        },
    ) {
        eprintln!("warning: failed to save history: {e}");
    }

    Ok(ipc_response)
}

#[tauri::command]
pub async fn list_history(
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<HistoryEntry>, String> {
    let col_path = state.collection_path.lock().await;
    let history_path = history::resolve_history_path(col_path.as_deref());
    drop(col_path);
    history::load_history(&history_path, limit.unwrap_or(50) as usize).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    let col_path = state.collection_path.lock().await;
    let history_path = history::resolve_history_path(col_path.as_deref());
    drop(col_path);
    history::clear_history(&history_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_collection_cmd(
    name: String,
    parent_dir: String,
    state: State<'_, AppState>,
) -> Result<IpcCollectionInfo, String> {
    let parent = Path::new(&parent_dir);
    let collection = create_collection(parent, &name).map_err(|e| e.to_string())?;
    let wire_dir = parent.join(".wire");

    let info = IpcCollectionInfo {
        name: collection.metadata.name.clone(),
        version: collection.metadata.version,
        active_env: collection.metadata.active_env.clone(),
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
    };

    *state.collection_path.lock().await = Some(wire_dir.clone());
    *state.collection.lock().await = Some(collection);

    Ok(info)
}

#[tauri::command]
pub async fn rename_collection_cmd(
    wire_dir: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<IpcCollectionInfo, String> {
    let path = Path::new(&wire_dir);
    let collection = rename_collection(path, &new_name).map_err(|e| e.to_string())?;

    let info = IpcCollectionInfo {
        name: collection.metadata.name.clone(),
        version: collection.metadata.version,
        active_env: collection.metadata.active_env.clone(),
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
    };

    *state.collection_path.lock().await = Some(path.to_path_buf());
    *state.collection.lock().await = Some(collection);

    Ok(info)
}

#[tauri::command]
pub async fn scan_codebase(
    project_dir: String,
    output_dir: String,
) -> Result<IpcScanResult, String> {
    let project_path = Path::new(&project_dir);
    let output_path = Path::new(&output_dir);

    let (scan_result, collection) =
        scan::scan_and_create_collection(project_path, output_path).map_err(|e| e.to_string())?;

    let framework = format!("{:?}", scan_result.framework);

    let (collection_info, wire_dir) = match collection {
        Some(col) => {
            let wire_dir = output_path.join(".wire");
            let info = IpcCollectionInfo {
                name: col.metadata.name.clone(),
                version: col.metadata.version,
                active_env: col.metadata.active_env.clone(),
                requests: col
                    .requests
                    .iter()
                    .map(|(p, r)| IpcRequestEntry {
                        path: p.to_string_lossy().to_string(),
                        name: r.name.clone(),
                        method: r.method.clone(),
                    })
                    .collect(),
                environments: col.environments.keys().cloned().collect(),
            };
            (Some(info), Some(wire_dir.to_string_lossy().to_string()))
        }
        None => (None, None),
    };

    Ok(IpcScanResult {
        framework,
        endpoints_found: scan_result.endpoints.len(),
        files_scanned: scan_result.files_scanned,
        collection: collection_info,
        wire_dir,
    })
}

#[tauri::command]
pub async fn get_environment(
    wire_dir: String,
    env_name: String,
) -> Result<std::collections::HashMap<String, String>, String> {
    let env_path = Path::new(&wire_dir)
        .join("envs")
        .join(format!("{env_name}.yaml"));
    if !env_path.exists() {
        return Err(format!(
            "Environment file not found: {}",
            env_path.display()
        ));
    }
    let content = std::fs::read_to_string(&env_path).map_err(|e| e.to_string())?;
    let env: Environment = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
    Ok(env.variables)
}

#[tauri::command]
pub async fn save_environment(
    wire_dir: String,
    env_name: String,
    variables: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let env_path = Path::new(&wire_dir)
        .join("envs")
        .join(format!("{env_name}.yaml"));

    // Read existing env to preserve the name field, or create new
    let name = if env_path.exists() {
        let content = std::fs::read_to_string(&env_path).map_err(|e| e.to_string())?;
        let existing: Environment = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
        existing.name
    } else {
        // Create envs directory if needed
        if let Some(parent) = env_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        env_name.clone()
    };

    let env = Environment { name, variables };
    let yaml = serde_yaml::to_string(&env).map_err(|e| e.to_string())?;
    std::fs::write(&env_path, yaml).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn read_request(file: String, state: State<'_, AppState>) -> Result<WireRequest, String> {
    let col_path = state.collection_path.lock().await;
    if let Some(ref wire_dir) = *col_path {
        load_request_resolved(Path::new(&file), wire_dir).map_err(|e| e.to_string())
    } else {
        load_request(Path::new(&file)).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn list_templates_cmd(wire_dir: String) -> Result<Vec<String>, String> {
    list_templates(Path::new(&wire_dir)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_request(path: String, request: WireRequest) -> Result<(), String> {
    let file_path = Path::new(&path);

    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directories: {e}"))?;
    }

    let yaml = serde_yaml::to_string(&request).map_err(|e| e.to_string())?;
    std::fs::write(file_path, yaml).map_err(|e| format!("Failed to write file: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn evaluate_tests(
    assertions: Vec<Assertion>,
    response: IpcResponse,
) -> Result<Vec<wire_core::test::TestResult>, String> {
    // Convert IpcResponse back to WireResponse for the evaluation engine
    let wire_response = wire_core::http::WireResponse {
        status: response.status,
        status_text: response.status_text,
        headers: response.headers,
        body: response.body,
        elapsed: std::time::Duration::from_millis(response.elapsed_ms),
        size_bytes: response.size_bytes,
    };

    Ok(wire_core::test::evaluate_assertions(
        &assertions,
        &wire_response,
    ))
}
