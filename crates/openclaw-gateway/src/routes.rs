use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;

use openclaw_core::types::Conversation;

use crate::AppState;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/api/conversations", post(create_conversation))
        .route("/api/conversations", get(list_conversations))
        .route("/api/conversations/{id}", get(get_conversation))
        .route("/api/conversations/{id}", axum::routing::delete(delete_conversation))
        .route("/api/health", get(health))
}

async fn health() -> &'static str {
    "ok"
}

async fn create_conversation(
    State(state): State<AppState>,
) -> Result<Json<Conversation>, StatusCode> {
    state
        .store
        .conversations()
        .create()
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn list_conversations(
    State(state): State<AppState>,
) -> Result<Json<Vec<Uuid>>, StatusCode> {
    state
        .store
        .conversations()
        .list_ids()
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Conversation>, StatusCode> {
    state
        .store
        .conversations()
        .get(id)
        .map(Json)
        .map_err(|_| StatusCode::NOT_FOUND)
}

async fn delete_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    state
        .store
        .conversations()
        .delete(id)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
