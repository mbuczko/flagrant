use serde::{Deserialize, Serialize};
use serde_valid::Validate;

extern crate regex;

pub mod payloads;
pub mod tabular;

mod internals;

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
    pub is_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Variant {
    #[sqlx(rename = "variant_id")]
    pub id: u16,
    pub value: FeatureValue,
    pub weight: u8,
    pub accumulator: i32,
    pub environment_id: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FeatureValue {
    Text(String),
    Json(String),
    Toml(String),
}

impl Feature {
    pub fn get_default_variant(&self) -> Option<&Variant> {
        self.variants.iter().find(|v| v.is_control())
    }
    pub fn get_default_value(&self) -> Option<&FeatureValue> {
        if let Some(variant) = self.get_default_variant() {
            return Some(&variant.value);
        }
        None
    }
    pub fn with_variants(mut self, variants: Vec<Variant>) -> Self {
        self.variants = variants;
        self
    }
}

impl Variant {
    pub fn build(id: u16, value: FeatureValue, weight: u8) -> Variant {
        Variant {
            id,
            value,
            weight,
            accumulator: weight as i32,
            environment_id: None,
        }
    }
    pub fn build_default(environment: &Environment, id: u16, value: FeatureValue) -> Variant {
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
