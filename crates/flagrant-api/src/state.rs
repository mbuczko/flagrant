use std::sync::{Arc, RwLock};

use axum::extract::FromRef;
use sqlx::SqlitePool;

#[cfg(feature = "redis")]
use crate::cache::FeatureCache;
use crate::config::ServerConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<RwLock<ServerConfig>>,
    #[cfg(feature = "redis")]
    pub cache: Option<Arc<FeatureCache>>,
}

impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}
