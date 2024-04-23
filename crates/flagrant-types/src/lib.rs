use serde::{Deserialize, Serialize};
use std::fmt;

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
    pub value: Option<(String, FeatureValueType)>,
    pub is_enabled: bool,
}

#[derive(Default, Debug, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "value_type", rename_all = "lowercase")]
// #[serde(rename_all = "lowercase")]
pub enum FeatureValueType {
    #[default]
    Text,
    Json,
    Toml,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, sqlx::FromRow)]
pub struct Variant {
    #[sqlx(rename = "variant_id")]
    pub id: u16,
    pub value: String,
    pub weight: i16,
    pub acc: i16,
}

#[derive(Serialize, Deserialize)]
pub struct EnvRequestPayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct FeatureRequestPayload {
    pub name: String,
    pub value: Option<(String, FeatureValueType)>,
    pub description: Option<String>,
    pub is_enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct VariantRequestPayload {
    pub value: String,
    pub weight: i16,
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let missing = "(missing)";
        let toggled = if self.is_enabled { "✓" } else { "☐" };
        let val = self
            .value
            .as_ref()
            .map(|(v, t)| (v.as_str(), t.to_string().to_lowercase()))
            .unwrap_or_else(|| (missing, missing.into()));

        write!(
            f,
            "│ {:<8}: {}\n│ {:<8}: {}\n│ {:<8}: {toggled} {}\n│ {:<8}: {}\n│ {:<8}: {}",
            "ID", self.id,
            "NAME", self.name,
            "ENABLED", self.is_enabled,
            "TYPE", val.1,
            "VALUE", val.0
        )
    }
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "│ {:<8}: {}\n│ {:<8}: {}\n│ {:<8}: {}",
            "ID", self.id,
            "WEIGHT", self.weight,
            "VALUE", self.value
        )
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
            _ => Self::default(),
        }
    }
}
