use std::{env, path::Path, sync::Arc};

use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tracing::init_tracing;

use crate::{config::ServerConfig, state::AppState};

mod api;
mod config;
mod errors;
mod extractors;
mod handlers;
mod openapi;
mod routes;
mod state;
mod tracing;

/// Loads the srv-token config from `FLAGRANT_CONFIG` (or `flagrant.toml` if unset).
/// Missing the default path is fine - it just means no srv-only feature is ever exposed
/// publicly. An explicitly configured path that fails to load is a startup error.
fn load_config() -> ServerConfig {
    match env::var("FLAGRANT_CONFIG") {
        Ok(path) => ServerConfig::load(Path::new(&path)).expect("Cannot load FLAGRANT_CONFIG"),
        Err(_) => {
            let default_path = Path::new("flagrant.toml");
            if default_path.exists() {
                ServerConfig::load(default_path).expect("Cannot load flagrant.toml")
            } else {
                ServerConfig::default()
            }
        }
    }
}

#[tokio::main]
async fn main() {
    init_tracing();

    let pool = flagrant::db::init_pool()
        .await
        .expect("Cannot initialize DB");
    let state = AppState {
        pool,
        config: Arc::new(load_config()),
    };
    let router = routes::init_router()
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3030")
        .await
        .expect("Cannot listen on port 3030");

    ::tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, router)
        .await
        .expect("Cannot start HTTP server");
}
