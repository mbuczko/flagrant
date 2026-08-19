use std::sync::{Arc, RwLock};

use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tracing::init_tracing;

use crate::{cache::FeatureCache, config::ServerConfig, state::AppState};

mod api;
mod cache;
mod config;
mod errors;
mod extractors;
mod handlers;
mod openapi;
mod routes;
mod state;
mod tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let pool = flagrant::db::init_pool()
        .await
        .expect("Cannot initialize DB");
    let config = ServerConfig::load_resolved().expect("Cannot load configuration");
    let cache = match &config.redis {
        Some(redis_config) => Some(Arc::new(
            FeatureCache::connect(redis_config)
                .await
                .expect("Cannot connect to Redis"),
        )),
        None => None,
    };
    let state = AppState {
        pool,
        config: Arc::new(RwLock::new(config)),
        cache,
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
