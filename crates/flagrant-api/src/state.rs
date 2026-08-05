use std::sync::{Arc, RwLock};

use axum::extract::FromRef;
use sqlx::SqlitePool;

use crate::config::ServerConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<RwLock<ServerConfig>>,
}

impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}
