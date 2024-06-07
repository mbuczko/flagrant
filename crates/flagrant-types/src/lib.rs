use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_valid::Validate;
use std::{fmt::{self, Display}, marker::PhantomData};

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
pub struct Feature<T: FeatureValueType> {
    #[sqlx(rename = "value_type")]
    pub _type: PhantomData<T>,

    #[sqlx(rename = "feature_id")]
    pub id: u16,
    pub project_id: u16,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
    pub variants: Vec<Variant<T>>,
    pub is_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureValue<T: FeatureValueType>(T, String);

pub trait FeatureValueType {
    fn discriminator(&self) -> &'static str;
}
pub struct JsonType {}
pub struct TomlType {}
pub struct TextType {}

impl FeatureValueType for JsonType {
    fn discriminator(&self) -> &'static str {
        "json"
    }
}
impl FeatureValueType for TomlType {
    fn discriminator(&self) -> &'static str {
        "toml"
    }
}
impl FeatureValueType for TextType {
    fn discriminator(&self) -> &'static str {
        "text"
    }
}

// #[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
// #[sqlx(type_name = "value_type", rename_all = "lowercase")]
// // #[serde(rename_all = "lowercase")]
// #[derive(Debug)]
// pub enum StringType {
//     // #[default]
//     Text,
//     Json,
//     Toml,
// }


// #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// pub struct FeatureValue(pub String, pub FeatureValueType);

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Variant<T: FeatureValueType> {
    #[sqlx(rename = "variant_id")]
    pub id: u16,
    pub value: FeatureValue<T>,
    pub weight: i16,
    pub accumulator: i32,
    pub environment_id: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvRequestPayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeatureRequestPayload<T: FeatureValueType> {
    pub name: String,
    pub value: Option<FeatureValue<T>>,
    pub description: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VariantRequestPayload {
    pub value: String,
    pub weight: i16,
}

impl<T: FeatureValueType> From<Feature<T>> for FeatureRequestPayload<T> {
    fn from(feature: Feature<T>) -> Self {
        FeatureRequestPayload {
            name: feature.name,
            value: feature
                .variants
                .into_iter()
                .find(|v| v.environment_id.is_some())
                .map(|v| v.value),
            description: None,
            is_enabled: feature.is_enabled,
        }
    }
}

impl<T: FeatureValueType> Feature<T> {
    pub fn get_default_variant(&self) -> Option<&Variant<T>> {
        self.variants.iter().find(|v| v.is_control())
    }
    pub fn get_default_value(&self) -> Option<FeatureValue<T>> {
        if let Some(variant) = self.get_default_variant() {
            return Some(variant.value);
        }
        None
    }
    pub fn with_variants(mut self, variants: Vec<Variant<T>>) -> Self {
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

// impl fmt::Display for FeatureValueType {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(f, "{:?}", self)
//     }
// }

// impl From<&str> for FeatureValueType {
//     fn from(value: &str) -> Self {
//         match value {
//             "json" => Self::Json,
//             "toml" => Self::Toml,
//             _ => Self::default(),
//         }
//     }
// }

pub trait Tabular {
    fn tabular_print(&self);
}

impl<T: FeatureValueType> Tabular for Feature<T> {
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

impl<T: FeatureValueType> Tabular for Variant<T> {
    fn tabular_print(&self) {
        println!(
            "│ {:<8}: {}\n│ {:<8}: {}\n│ {:<8}: {}",
            "ID", self.id, "WEIGHT", self.weight, "VALUE", self.value
        )
    }
}
