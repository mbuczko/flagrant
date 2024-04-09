use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    #[sqlx(rename = "project_id")]
    pub id: u16,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Environment {
    #[sqlx(rename = "environment_id")]
    pub id: u16,
    pub project_id: u16,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Feature {
    #[sqlx(rename = "feature_id")]
    pub id: u16,
    pub project_id: u16,
    pub name: String,
    pub value: Option<String>,
    pub value_type: FeatureValueType,
    pub is_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "value_type", rename_all = "lowercase")]
// #[serde(rename_all = "lowercase")]
pub enum FeatureValueType {
    Text,
    Json,
    Toml,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, sqlx::FromRow)]
pub struct Variant {
    #[sqlx(rename = "variant_id")]
    pub id: u16,
    pub value: String,
    pub weight: u16,
    pub acc: i16,
}

#[derive(Serialize, Deserialize)]
pub struct NewEnvRequestPayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct NewFeatureRequestPayload {
    pub name: String,
    pub value: Option<String>,
    pub value_type: FeatureValueType,
    pub description: Option<String>,
    pub is_enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct NewVariantRequestPayload {
    pub value: String,
    pub weight: u16,
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let vstr = self
            .value
            .as_ref()
            .map(|s| format!("{s} [type={}]", self.value_type))
            .unwrap_or_else(|| "(missing)".into());

        write!(
            f,
            "id={}, name={}, value={vstr}, is_enabled={}",
            self.id, self.name, self.is_enabled
        )
    }
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "id={}, weight={}, value={}", self.id, self.weight, self.value)
    }
}

impl fmt::Display for FeatureValueType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<&str> for FeatureValueType {
    fn from(value: &str) -> Self {
        match value {
            "json" => Self::Json,
            "toml" => Self::Toml,
            _ => Self::Text,
        }
    }
}
