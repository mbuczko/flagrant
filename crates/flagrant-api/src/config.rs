use std::{collections::HashMap, fs, path::Path};

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

    pub fn srv_token(&self, project: &str, environment: &str) -> Option<&str> {
        self.projects
            .get(project)?
            .envs
            .get(environment)?
            .srv_token
            .as_deref()
    }
}
