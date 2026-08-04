use axum::{Json, extract::State};
use flagrant::errors::FlagrantError;
use serde_json::{Value, json};

use crate::{config::ServerConfig, errors::ServiceError, state::AppState};

/// Re-reads server configuration (currently just srv-tokens) from disk, replacing
/// whatever was loaded at startup or on a previous reload. Lets a token be added,
/// changed or removed without restarting the server.
#[utoipa::path(
    post,
    path = "/admin/reload",
    responses(
        (status = 200, description = "Configuration reloaded"),
    ),
    tag = "admin"
)]
pub async fn reload_config(State(state): State<AppState>) -> Result<Json<Value>, ServiceError> {
    let config = ServerConfig::load_resolved()
        .map_err(|e| FlagrantError::UnexpectedFailure("Could not reload configuration", e))?;

    *state.config.write().unwrap() = config;

    tracing::info!("Configuration reloaded");
    Ok(Json(json!({ "message": "Configuration reloaded" })))
}
