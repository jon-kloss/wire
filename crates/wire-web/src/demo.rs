//! The bundled demo API — the only target the playground sends requests to.
//!
//! A tiny in-memory pet store plus a few httpbin-style utility endpoints, enough
//! to exercise sending requests, assertions, and request chaining without ever
//! reaching out to the public internet.

use crate::state::SharedState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/health", get(health))
        .route("/headers", get(headers))
        .route("/status/{code}", get(status))
        .route("/pets", get(list_pets).post(create_pet))
        .route("/pets/{id}", get(get_pet))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn headers(headers: HeaderMap) -> Json<Value> {
    let map: HashMap<String, String> = headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    Json(json!({ "headers": map }))
}

async fn status(Path(code): Path<u16>) -> impl IntoResponse {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);
    (status, Json(json!({ "status": code })))
}

async fn list_pets(State(state): State<SharedState>) -> Json<Value> {
    let pets = state.demo_pets.lock().await;
    Json(Value::Array(pets.clone()))
}

async fn get_pet(State(state): State<SharedState>, Path(id): Path<u64>) -> impl IntoResponse {
    let pets = state.demo_pets.lock().await;
    match pets
        .iter()
        .find(|p| p.get("id").and_then(Value::as_u64) == Some(id))
    {
        Some(pet) => (StatusCode::OK, Json(pet.clone())),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "pet not found" })),
        ),
    }
}

async fn create_pet(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let mut pets = state.demo_pets.lock().await;
    let next_id = pets.len() as u64 + 1;
    let mut pet = body;
    if let Value::Object(ref mut map) = pet {
        map.insert("id".to_string(), json!(next_id));
    }
    pets.push(pet.clone());
    (StatusCode::CREATED, Json(pet))
}
