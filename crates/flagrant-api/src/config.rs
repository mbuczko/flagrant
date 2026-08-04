use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

/// Server configuration loaded once at startup from a TOML file, keyed by project and
/// environment name. Currently only carries the `srv-token` unlocking server-side-only
/// features, but the nesting leaves room for further per-environment settings later.
///
/// ```toml
/// [projects.my_project.envs.production]
/// srv-token = "prod-secret-token"
/// ```
#[derive(Debug, Default, Deserialize)]
pub struct ServerConfig {
    #[serde(default)]
    pub projects: HashMap<String, ProjectConfig>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub envs: HashMap<String, EnvironmentConfig>,
}

#[derive(Debug, Default, Deserialize)]
pub struct EnvironmentConfig {
    #[serde(rename = "srv-token")]
    pub srv_token: Option<String>,
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
            .envs
            .get(environment)?
            .srv_token
            .as_deref()
    }
}
