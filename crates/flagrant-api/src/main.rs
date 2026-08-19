use std::sync::{Arc, RwLock};

use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tracing::init_tracing;

#[cfg(feature = "redis")]
use crate::cache::FeatureCache;
use crate::{config::ServerConfig, state::AppState};

mod api;
#[cfg(feature = "redis")]
mod cache;
mod config;
mod errors;
mod extractors;
#[cfg(feature = "grpc")]
mod grpc;
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

    #[cfg(feature = "redis")]
    let cache = match &config.redis {
        Some(redis_config) => Some(Arc::new(
            FeatureCache::connect(redis_config)
                .await
                .expect("Cannot connect to Redis"),
        )),
        None => None,
    };

    // Grabbed before `config` is moved into the Arc<RwLock<_>> below - the gRPC listener
    // address is only ever read once, at startup, unlike srv-token/cache config which is
    // re-read from `state.config` on every request and can be hot-reloaded via
    // `/admin/reload` (a bound listener can't be rebound onto a new address in place).
    #[cfg(feature = "grpc")]
    let grpc_config = config.grpc.clone();

    let state = AppState {
        pool,
        config: Arc::new(RwLock::new(config)),
        #[cfg(feature = "redis")]
        cache,
    };

    #[cfg(feature = "grpc")]
    if let Some(grpc_config) = grpc_config {
        let grpc_state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = grpc::serve(grpc_config, grpc_state).await {
                ::tracing::error!(error = ?err, "gRPC server exited with an error");
            }
        });
    }

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
