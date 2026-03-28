use crate::state::AppState;
use crate::types::{IpcCollectionInfo, IpcRequestEntry, IpcResponse};
use std::path::Path;
use tauri::State;
use wire_core::collection::{load_collection, load_request, WireRequest};
use wire_core::http::execute;
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
    let request = load_request(Path::new(&file)).map_err(|e| e.to_string())?;

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

    Ok(IpcResponse::from(response))
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

    Ok(IpcResponse::from(response))
}

#[tauri::command]
pub async fn read_request(file: String) -> Result<WireRequest, String> {
    load_request(Path::new(&file)).map_err(|e| e.to_string())
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
