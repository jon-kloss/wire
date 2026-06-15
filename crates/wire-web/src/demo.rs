//! The bundled demo API — the only target the playground sends requests to.
//!
//! A tiny in-memory pet store plus a few httpbin-style utility endpoints, enough
//! to exercise sending requests, assertions, and request chaining without ever
//! reaching out to the public internet.
//!
//! Every route is scoped under `/s/{sid}` so each visitor session gets its own
//! isolated state. The session id is baked into each session's `{{base_url}}`
//! when its sandbox is seeded, so requests land on the right store automatically.

use crate::state::{seed_demo_pets, SharedState};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Per-session cap on stored demo pets, to bound `POST /pets` growth.
const MAX_DEMO_PETS: usize = 100;

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/s/{sid}/health", get(health))
        .route("/s/{sid}/headers", get(headers))
        .route("/s/{sid}/status/{code}", get(status))
        .route("/s/{sid}/pets", get(list_pets).post(create_pet))
        .route("/s/{sid}/pets/{id}", get(get_pet))
}

async fn health(Path(_sid): Path<String>) -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn headers(Path(_sid): Path<String>, headers: HeaderMap) -> Json<Value> {
    let map: HashMap<String, String> = headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    Json(json!({ "headers": map }))
}

async fn status(Path((_sid, code)): Path<(String, u16)>) -> impl IntoResponse {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);
    (status, Json(json!({ "status": code })))
}

async fn list_pets(State(state): State<SharedState>, Path(sid): Path<String>) -> Json<Value> {
    let mut store = state.demo_pets.lock().await;
    let pets = store.entry(sid).or_insert_with(seed_demo_pets);
    Json(Value::Array(pets.clone()))
}

async fn get_pet(
    State(state): State<SharedState>,
    Path((sid, id)): Path<(String, u64)>,
) -> impl IntoResponse {
    let mut store = state.demo_pets.lock().await;
    let pets = store.entry(sid).or_insert_with(seed_demo_pets);
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
    Path(sid): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let mut store = state.demo_pets.lock().await;
    let pets = store.entry(sid).or_insert_with(seed_demo_pets);
    let next_id = pets
        .last()
        .and_then(|p| p.get("id"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    let mut pet = body;
    if let Value::Object(ref mut map) = pet {
        map.insert("id".to_string(), json!(next_id));
    }
    pets.push(pet.clone());
    // Bound growth: drop the oldest entries past the cap.
    if pets.len() > MAX_DEMO_PETS {
        let overflow = pets.len() - MAX_DEMO_PETS;
        pets.drain(0..overflow);
    }
    (StatusCode::CREATED, Json(pet))
}
