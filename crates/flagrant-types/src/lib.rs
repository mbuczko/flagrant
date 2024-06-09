use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use sqlx::{
    encode::IsNull,
    sqlite::{SqliteArgumentValue, SqliteValueRef},
    Decode, Encode, Sqlite, Type,
};
use std::{fmt, str::FromStr};
use thiserror::Error;

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
    pub is_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FeatureValue {
    Text(String),
    Json(String),
    Toml(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Variant {
    #[sqlx(rename = "variant_id")]
    pub id: u16,
    pub value: FeatureValue,
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
                .into_iter()
                .find(|v| v.environment_id.is_some())
                .map(|v| v.value),
            description: None,
            is_enabled: feature.is_enabled,
        }
    }
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
    pub fn build(id: u16, value: FeatureValue, weight: i16) -> Variant {
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

#[derive(Debug, Error)]
pub enum ParseTypeError {
    #[error("'{0}' is an unknown value type")]
    Type(String),

    #[error("Value incorrectly encoded")]
    Encoding,
}

impl sqlx::Type<sqlx::Sqlite> for FeatureValue {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl FromStr for FeatureValue {
    type Err = ParseTypeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some((typ, val)) = value.split_once("::") {
            return Self::new(typ, val);
        }
        Err(ParseTypeError::Encoding)
    }
}

impl Encode<'_, Sqlite> for FeatureValue {
    fn encode_by_ref(&self, buf: &mut Vec<SqliteArgumentValue<'_>>) -> IsNull {
        Encode::<Sqlite>::encode(self.to_string(), buf)
    }
}

impl<'r> Decode<'r, Sqlite> for FeatureValue {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let value = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        Self::from_str(value).map_err(Into::into)
    }
}

impl Default for FeatureValue {
    fn default() -> Self {
        Self::Text(String::default())
    }
}

impl fmt::Display for FeatureValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (typ, val) = self.decompose();
        write!(f, "{typ}::{}", val.trim())
    }
}

impl FeatureValue {
    pub fn new(typ: &str, value: &str) -> Result<Self, ParseTypeError> {
        let val = value.to_owned();
        match typ {
            "json" => Ok(Self::Json(val)),
            "toml" => Ok(Self::Toml(val)),
            "text" => Ok(Self::Text(val)),
            _ => Err(ParseTypeError::Type(typ.to_owned())),
        }
    }
    pub fn decompose(&self) -> (&str, &str) {
        match self {
            Self::Json(v) => ("json", v),
            Self::Toml(v) => ("toml", v),
            Self::Text(v) => ("text", v),
        }
    }
}

pub trait Tabular {
    fn tabular_print(&self);
}

impl Tabular for Feature {
    fn tabular_print(&self) {
        let toggle = if self.is_enabled { "▣" } else { "▢" };
        let value = self.get_default_variant().map(|v| &v.value);

        println!(
            "│ {:<8}: {}\n│ {:<8}: {}\n│ {:<8}: {toggle} {}\n│ {:<8}: {}",
            "ID",
            self.id,
            "NAME",
            self.name,
            "ENABLED",
            self.is_enabled,
            "VALUE",
            value
                .map(|v| v.to_string())
                .unwrap_or_default()
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
