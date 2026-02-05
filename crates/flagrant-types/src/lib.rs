use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use sqlx::{Decode, Encode, Sqlite, Type, encode::IsNull, sqlite::SqliteValueRef};
use std::{fmt, str::FromStr};
use thiserror::Error;

extern crate regex;

pub mod payload;

#[derive(Debug, Error)]
pub enum ParseTypeError {
    #[error("'{0}' is an unknown value type")]
    Type(String),

    #[error("Value incorrectly encoded")]
    Encoding,
}

#[derive(Debug, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    #[sqlx(rename = "project_id")]
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct Environment {
    #[sqlx(rename = "environment_id")]
    pub id: i32,
    pub project_id: i32,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Validate)]
pub struct Feature {
    #[sqlx(rename = "feature_id")]
    pub id: i32,
    pub project_id: i32,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
    pub variants: Vec<Variant>,
    pub tags: TagList,
    pub is_enabled: bool,
    pub is_active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Variant {
    #[sqlx(rename = "variant_id")]
    pub id: i32,
    pub value: FeatureValue,
    pub weight: u8,
    pub accumulator: i32,
    pub environment_id: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct IdentityVariant {
    pub variant_id: i32,
    pub feature_id: i32,
    pub identity_id: Option<i32>,
    pub migrated_id: Option<i32>,
    pub name: String,
    pub value: FeatureValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FeatureValue {
    Text(String),
    Json(String),
    Toml(String),
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct TagList(pub Vec<Tag>);

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tag {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeatureResponse {
    pub feature_id: i32,
    pub feature_name: String,
    pub value: FeatureValue,
}

impl Feature {
    pub fn get_default_variant(&self) -> &Variant {
        self.variants
            .iter()
            .find(|v| v.is_control())
            .expect("Feature has no default variant!")
    }
    pub fn get_default_value(&self) -> &FeatureValue {
        &self.get_default_variant().value
    }
    pub fn with_variants(mut self, variants: Vec<Variant>) -> Self {
        self.variants = variants;
        self
    }
}

impl Variant {
    pub fn build(id: i32, value: FeatureValue, weight: u8) -> Variant {
        Variant {
            id,
            value,
            weight,
            accumulator: weight as i32,
            environment_id: None,
        }
    }
    pub fn build_default(environment: &Environment, id: i32, value: FeatureValue) -> Variant {
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

impl sqlx::Type<sqlx::Sqlite> for TagList {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl sqlx::Type<sqlx::Sqlite> for FeatureValue {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

// It's not really used. Tags are are normalized and stored in separate table
// but since entire Feature is Serialize, TagList needs to be Serialize too.
impl Encode<'_, Sqlite> for TagList {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        if self.0.is_empty() {
            Ok(IsNull::Yes)
        } else {
            Encode::<Sqlite>::encode(
                self.0
                    .iter()
                    .map(|tag| tag.name.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                buf,
            )
        }
    }
}

impl<'r> Decode<'r, Sqlite> for TagList {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let value = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        if !value.is_empty() {
            let tags: Vec<Tag> = value
                .split(',')
                .filter_map(|tag| {
                    let name = tag.trim().to_string();
                    if name.is_empty() {
                        return None;
                    }
                    Some(Tag { name })
                })
                .collect();
            return Ok(TagList(tags));
        }
        Ok(TagList(Vec::new()))
    }
}

impl fmt::Display for TagList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|tag| tag.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl Encode<'_, Sqlite> for FeatureValue {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
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
    fn new(typ: &str, value: &str) -> Result<Self, ParseTypeError> {
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
    pub fn build(value: &str) -> Self {
        let val = value.trim();
        match val.chars().next() {
            Some('{') => Self::Json(val.to_owned()),
            Some('[') => Self::Toml(val.to_owned()),
            _ => Self::Text(val.to_owned()),
        }
    }
    pub fn clone_with(&self, value: &str) -> Self {
        let (typ, _) = self.decompose();
        Self::new(typ, value).unwrap()
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
