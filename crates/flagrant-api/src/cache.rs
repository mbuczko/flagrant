use std::time::Duration;

use flagrant_types::VariantValue;
use redis::{AsyncCommands, aio::ConnectionManager};
use serde::{Deserialize, Serialize};

use crate::config::RedisConfig;

/// Wire format stored in Redis for one (project, environment, identity) entry -
/// the full resolved feature list, srv-only features included. Filtering by
/// `is_srv` happens at read time, same as the uncached DB path, so one entry
/// serves both privileged and unprivileged callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFeature {
    pub feature_id: i32,
    pub name: String,
    pub value: VariantValue,
    pub is_srv: bool,
    pub is_enabled: bool,
}

pub struct FeatureCache {
    conn: ConnectionManager,
    ttl: Duration,
}

impl FeatureCache {
    pub async fn connect(config: &RedisConfig) -> anyhow::Result<Self> {
        let client = redis::Client::open(config.url.as_str())?;
        let conn = client.get_connection_manager().await?;

        tracing::info!("Connected to Redis instance (cache enabled)");

        Ok(Self {
            conn,
            ttl: Duration::from_secs(config.ttl_seconds),
        })
    }

    pub fn key(project: &str, environment: &str, identity: &str) -> String {
        format!("flagrant:features:{project}:{environment}:{identity}")
    }

    /// Best-effort lookup: any Redis or deserialization error is treated as a
    /// miss, never surfaced to the caller.
    pub async fn get(&self, key: &str) -> Option<Vec<CachedFeature>> {
        let mut conn = self.conn.clone();
        let raw: Option<String> = match conn.get(key).await {
            Ok(raw) => raw,
            Err(err) => {
                tracing::warn!("Redis GET failed, falling back to DB: {err}");
                return None;
            }
        };

        raw.and_then(|s| serde_json::from_str(&s).ok())
    }

    /// Best-effort write: a cache outage must never fail the request, only
    /// remove the speedup.
    pub async fn set(&self, key: &str, features: &[CachedFeature]) {
        let payload = match serde_json::to_string(features) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::warn!("Could not serialize features for caching: {err}");
                return;
            }
        };

        let mut conn = self.conn.clone();
        if let Err(err) = conn
            .set_ex::<_, _, ()>(key, payload, self.ttl.as_secs())
            .await
        {
            tracing::warn!("Redis SET failed: {err}");
        }
    }
}
