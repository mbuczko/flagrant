use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use std::fmt;

extern crate regex;

#[derive(Debug, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    #[sqlx(rename = "project_id")]
    pub id: u16,
    pub name: String,
}

#[derive(Debug, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct Environment {
    #[sqlx(rename = "environment_id")]
    pub id: u16,
    pub project_id: u16,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Validate)]
pub struct Feature {
    #[sqlx(rename = "feature_id")]
    pub id: u16,
    pub project_id: u16,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
    pub variants: Vec<Variant>,
    pub value_type: FeatureValueType,
    pub is_enabled: bool,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "value_type", rename_all = "lowercase")]
// #[serde(rename_all = "lowercase")]
pub enum FeatureValueType {
    #[default]
    Text,
    Json,
    Toml,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureValue(pub String, pub FeatureValueType);

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, sqlx::FromRow)]
pub struct Variant {
    #[sqlx(rename = "variant_id")]
    pub id: u16,
    pub value: String,
    pub weight: i16,
    pub accumulator: i16,
    pub environment_id: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvRequestPayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeatureRequestPayload {
    pub name: String,
    pub value: Option<FeatureValue>,
    pub description: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VariantRequestPayload {
    pub value: String,
    pub weight: i16,
}

impl From<Feature> for FeatureRequestPayload {
    fn from(feature: Feature) -> Self {
        FeatureRequestPayload {
            name: feature.name,
            value: feature
                .variants
                .first()
                .map(|v| FeatureValue(v.value.clone(), feature.value_type)),
            description: None,
            is_enabled: feature.is_enabled,
        }
    }
}

impl Feature {
    pub fn get_default_variant(&self) -> Option<&Variant> {
        self.variants.iter().find(|v| v.is_control())
    }
    pub fn get_default_value(&self) -> Option<FeatureValue> {
        if let Some(variant) = self.get_default_variant() {
            return Some(FeatureValue(variant.value.clone(), self.value_type.clone()));
        }
        None
    }
    pub fn with_variants(mut self, variants: Vec<Variant>) -> Self {
        self.variants = variants;
        self
    }
}

impl Variant {
    pub fn build(id: u16, value: String, weight: i16) -> Variant {
        Variant {
            id,
            value,
            weight,
            accumulator: 100,
            environment_id: None,
        }
    }
    pub fn build_default(environment: &Environment, id: u16, value: String) -> Variant {
        Variant {
            id,
            value,
            weight: 100,
            accumulator: 100,
            environment_id: Some(environment.id),
        }
    }
    pub fn is_control(&self) -> bool {
        self.environment_id.is_some()
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

pub trait Tabular {
    fn tabular_print(&self);
}

impl Tabular for Feature {
    fn tabular_print(&self) {
        let toggle = if self.is_enabled { "▣" } else { "▢" };
        let value = self
            .get_default_variant()
            .map(|v| v.value.as_str())
            .unwrap_or_else(|| "");

        println!(
            "│ {:<8}: {}\n│ {:<8}: {}\n│ {:<8}: {toggle} {}\n│ {:<8}: {}\n│ {:<8}: {}",
            "ID",
            self.id,
            "NAME",
            self.name,
            "ENABLED",
            self.is_enabled,
            "TYPE",
            self.value_type,
            "VALUE",
            value
        )
    }
}

impl Tabular for Variant {
    fn tabular_print(&self) {
        println!(
            "│ {:<8}: {}\n│ {:<8}: {}\n│ {:<8}: {}",
            "ID", self.id, "WEIGHT", self.weight, "VALUE", self.value
        )
    }
}
