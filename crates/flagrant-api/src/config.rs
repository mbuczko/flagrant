use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer};

/// Server configuration loaded once at startup from a TOML file, keyed by `<project>/<env>`
/// in the file but stored nested (project, then environment) so lookups are two plain `&str`
/// map accesses - no string concatenation/allocation needed.
///
/// Carries per-project-environment settings (currently just the `srv-token` unlocking
/// server-side-only features), an optional `[http]` section overriding the HTTP listen
/// address, plus optional top-level `[redis]` and `[grpc]` sections enabling those features
/// when present.
///
/// ```toml
/// [projects."my_project/production"]
/// srv-token = "prod-secret-token"
/// ```
#[derive(Debug, Default, Deserialize)]
pub struct ServerConfig {
    #[serde(default, deserialize_with = "deserialize_projects")]
    pub projects: HashMap<String, HashMap<String, ProjectEnvConfig>>,
    #[serde(default)]
    pub http: HttpConfig,
    #[serde(default)]
    pub redis: Option<RedisConfig>,
    #[serde(default)]
    pub grpc: Option<GrpcConfig>,
}

/// The always-on HTTP server's listen address. Unlike `[redis]`/`[grpc]`, there's nothing
/// to opt into here - `[http]` is entirely optional and just overrides the default
/// `listen` address when present; the HTTP route is always served either way.
///
/// Read once at startup, same as `[grpc]`'s `listen` - not picked up by `/admin/reload`,
/// since a bound listener can't be rebound onto a new address in place.
///
/// ```toml
/// [http]
/// listen = "0.0.0.0:3030"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct HttpConfig {
    #[serde(default = "default_http_listen")]
    pub listen: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            listen: default_http_listen(),
        }
    }
}

fn default_http_listen() -> String {
    "127.0.0.1:3030".to_owned()
}

/// Optional gRPC exposure of the public feature-resolution endpoint, alongside the
/// always-on HTTP route. Absent `[grpc]` section means the gRPC listener is disabled
/// entirely - same on/off convention as `[redis]`.
///
/// `listen` is either a plain `host:port` (TCP) or a `unix:<path>` for a Unix domain
/// socket (useful for same-host IPC without exposing a TCP port). Read once at startup;
/// unlike `srv-token`, it is *not* picked up by `/admin/reload` - a listener can't be
/// rebound onto a new address/path once the server is already running, so changing this
/// value requires a restart.
///
/// ```toml
/// [grpc]
/// listen = "127.0.0.1:50051"
/// # or: listen = "unix:/tmp/flagrant/grpc.sock"
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct GrpcConfig {
    pub listen: String,
}

/// Optional Redis-backed response cache for the public features endpoint. Absent
/// `[redis]` section means caching is disabled entirely.
///
/// ```toml
/// [redis]
/// url = "redis://127.0.0.1:6379"
/// ttl-seconds = 30
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: String,
    #[serde(rename = "ttl-seconds", default = "default_ttl_seconds")]
    pub ttl_seconds: u64,
}

fn default_ttl_seconds() -> u64 {
    30
}

#[derive(Debug, Default, Deserialize)]
pub struct ProjectEnvConfig {
    #[serde(rename = "srv-token")]
    pub srv_token: Option<String>,
}

/// Splits the file's flat `"<project>/<env>"` keys into a nested map at load time, so
/// [`ServerConfig::srv_token`] never has to allocate a combined key back together.
fn deserialize_projects<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, HashMap<String, ProjectEnvConfig>>, D::Error>
where
    D: Deserializer<'de>,
{
    let flat = HashMap::<String, ProjectEnvConfig>::deserialize(deserializer)?;
    let mut nested = HashMap::<String, HashMap<String, ProjectEnvConfig>>::new();

    for (key, config) in flat {
        let (project, environment) = key.split_once('/').ok_or_else(|| {
            serde::de::Error::custom(format!(
                "invalid `projects` key {key:?}: expected \"<project>/<environment>\""
            ))
        })?;
        nested
            .entry(project.to_owned())
            .or_default()
            .insert(environment.to_owned(), config);
    }

    Ok(nested)
}

impl ServerConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }

    /// Resolves the config path from `FLAGRANT_CONFIG`, falling back to `flagrant.toml`
    /// if it exists in the current directory. Returns `None` when neither is available.
    /// Re-checked on every call (not cached) so a file created after startup, or a
    /// `flagrant.toml` that appears later, is picked up on the next reload.
    pub fn resolve_path() -> Option<PathBuf> {
        match env::var("FLAGRANT_CONFIG") {
            Ok(path) => Some(PathBuf::from(path)),
            Err(_) => {
                let default_path = PathBuf::from("flagrant.toml");
                default_path.exists().then_some(default_path)
            }
        }
    }

    /// Loads configuration from [`Self::resolve_path`], defaulting to an empty config
    /// (no srv-tokens) when no file is found. Used both at server startup and whenever
    /// the CLI's `RELOAD` command asks the server to re-read its configuration.
    pub fn load_resolved() -> anyhow::Result<Self> {
        match Self::resolve_path() {
            Some(path) => Self::load(&path),
            None => Ok(Self::default()),
        }
    }

    pub fn srv_token(&self, project: &str, environment: &str) -> Option<&str> {
        self.projects
            .get(project)?
            .get(environment)?
            .srv_token
            .as_deref()
    }
}
