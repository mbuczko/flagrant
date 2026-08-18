use std::sync::{Arc, RwLock};

use axum::extract::FromRef;
use sqlx::SqlitePool;

use crate::{cache::FeatureCache, config::ServerConfig};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<RwLock<ServerConfig>>,
    pub cache: Option<Arc<FeatureCache>>,
}

impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}
