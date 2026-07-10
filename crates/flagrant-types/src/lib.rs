use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use sqlx::{Decode, Encode, Sqlite, Type, encode::IsNull, sqlite::SqliteValueRef};
use std::{fmt, str::FromStr};
use thiserror::Error;
use utoipa::ToSchema;

extern crate regex;

pub mod payload;

// max variant size is 1kb (1024 bytes)
const MAX_VARIANT_SIZE: usize = 1024;

#[derive(Debug, Error)]
pub enum ParseTypeError {
    #[error("'{0}' is an unknown value type")]
    Type(String),

    #[error("Value incorrectly encoded")]
    Encoding,

    #[error("Value exceeds max size of 1024 bytes")]
    SizeExceeded,
}

#[derive(Debug, Default, Serialize, Deserialize, sqlx::FromRow, Validate, ToSchema)]
pub struct Project {
    #[sqlx(rename = "project_id")]
    pub id: i32,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
}

#[derive(Debug, Default, Serialize, Deserialize, sqlx::FromRow, Validate, ToSchema)]
pub struct Environment {
    #[sqlx(rename = "environment_id")]
    pub id: i32,
    pub project_id: i32,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Validate, ToSchema)]
pub struct Feature {
    #[sqlx(rename = "feature_id")]
    pub id: i32,
    pub project_id: i32,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
    #[validate(max_length = 2048)]
    pub description: String,
    pub variants: Vec<Variant>,
    pub tags: TagList,
    pub is_enabled: bool,
    pub is_archived: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Variant {
    #[sqlx(rename = "variant_id")]
    pub id: i32,
    pub value: FeatureValue,
    pub weight: u8,
    pub accumulator: i32,
    pub environment_id: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Identity {
    pub id: i32,
    pub value: String,
    #[serde(skip)]
    pub environment_id: i32,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Trait {
    #[sqlx(rename = "trait_id")]
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum TraitValue {
    Str(String),
    Int(i32),
    Float(f32),
    Bool(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct IdentityTrait {
    pub trait_id: i32,
    pub name: String,
    pub value: Option<TraitValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IdentityWithTraits {
    pub id: i32,
    pub value: String,
    pub traits: Vec<IdentityTrait>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct IdentityVariant {
    pub variant_id: Option<i32>,
    pub feature_id: i32,
    pub identity_id: Option<i32>,
    pub migrated_id: Option<i32>,
    pub feature_name: String,
    pub feature_value: Option<FeatureValue>,
    pub pinned_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SegmentDriver {
    /// Match against the identity value string (e.g. email, user ID).
    Identity,
    /// Match against a named identity trait. The `String` is the trait name.
    Trait(String),
    /// Match against the environment name.
    Environment,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Comparator {
    ExactlyMatches,
    DoesNotMatch,
    Contains,
    DoesNotContain,
    GreaterThan,
    GreaterEqualThan,
    LowerThan,
    LowerEqualThan,
    /// Value must be a JSON array string, e.g. `["a","b"]`.
    In,
    /// Value must be a JSON array string.
    NotIn,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct SegmentRule {
    #[sqlx(rename = "rule_id")]
    pub id: i32,
    pub driver: SegmentDriver,
    pub comparator: Comparator,
    /// For `In`/`NotIn` comparators this is a JSON array string; otherwise a plain value.
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupConnector {
    And,
    AndNot,
}

impl sqlx::Type<Sqlite> for SegmentDriver {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}
impl Encode<'_, Sqlite> for SegmentDriver {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        let s = match self {
            Self::Identity => "identity".to_string(),
            Self::Trait(name) => format!("trait:{name}"),
            Self::Environment => "environment".to_string(),
        };
        Encode::<Sqlite>::encode(s, buf)
    }
}
impl<'r> Decode<'r, Sqlite> for SegmentDriver {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        match s {
            "identity" => Ok(Self::Identity),
            "environment" => Ok(Self::Environment),
            _ if s.starts_with("trait:") => Ok(Self::Trait(s[6..].to_string())),
            _ => Err(format!("Unknown segment driver: {s}").into()),
        }
    }
}

impl sqlx::Type<Sqlite> for Comparator {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}
impl Encode<'_, Sqlite> for Comparator {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        let s = match self {
            Self::ExactlyMatches => "exactly_matches",
            Self::DoesNotMatch => "does_not_match",
            Self::Contains => "contains",
            Self::DoesNotContain => "does_not_contain",
            Self::GreaterThan => "greater_than",
            Self::GreaterEqualThan => "greater_equal_than",
            Self::LowerThan => "lower_than",
            Self::LowerEqualThan => "lower_equal_than",
            Self::In => "in",
            Self::NotIn => "not_in",
        };
        Encode::<Sqlite>::encode(s, buf)
    }
}
impl<'r> Decode<'r, Sqlite> for Comparator {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        match s {
            "exactly_matches" => Ok(Self::ExactlyMatches),
            "does_not_match" => Ok(Self::DoesNotMatch),
            "contains" => Ok(Self::Contains),
            "does_not_contain" => Ok(Self::DoesNotContain),
            "greater_than" => Ok(Self::GreaterThan),
            "greater_equal_than" => Ok(Self::GreaterEqualThan),
            "lower_than" => Ok(Self::LowerThan),
            "lower_equal_than" => Ok(Self::LowerEqualThan),
            "in" => Ok(Self::In),
            "not_in" => Ok(Self::NotIn),
            _ => Err(format!("Unknown comparator: {s}").into()),
        }
    }
}

impl sqlx::Type<Sqlite> for GroupConnector {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}
impl Encode<'_, Sqlite> for GroupConnector {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        let s = match self {
            Self::And => "and",
            Self::AndNot => "and_not",
        };
        Encode::<Sqlite>::encode(s, buf)
    }
}
impl<'r> Decode<'r, Sqlite> for GroupConnector {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        match s {
            "and" => Ok(Self::And),
            "and_not" => Ok(Self::AndNot),
            _ => Err(format!("Unknown group connector: {s}").into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct SegmentGroup {
    #[sqlx(rename = "group_id")]
    pub id: i32,
    /// Stable auto-generated label (e.g. "group-1"). Never reassigned after deletion.
    pub label: String,
    pub description: Option<String>,
    /// `None` for the first (head) group; `Some` for all subsequent groups.
    pub connector: Option<GroupConnector>,
    pub rules: Vec<SegmentRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, Validate, ToSchema)]
pub struct Segment {
    #[sqlx(rename = "segment_id")]
    pub id: i32,
    pub project_id: i32,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
    #[validate(max_length = 2048)]
    pub description: Option<String>,
    /// Groups ordered by position; first group has `connector = None`.
    pub groups: Vec<SegmentGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum FeatureOverride {
    Identity(String),
    Segment {
        name: String,
        weights: Vec<payload::SegmentVariantWeight>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FeatureValue {
    Text(String),
    Json(String),
    Toml(String),
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
#[schema(value_type = Vec<Tag>)]
pub struct TagList(pub Vec<Tag>);

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Tag {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FeatureResponse {
    pub feature_id: i32,
    pub name: String,
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
        f.write_fmt(core::format_args!(
            "{}",
            self.0
                .iter()
                .map(|tag| tag.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ))
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
        Self::from_str(val).unwrap_or_else(|_| match val.chars().next() {
            Some('{') => Self::Json(val.to_owned()),
            Some('[') => Self::Toml(val.to_owned()),
            _ => Self::Text(val.to_owned()),
        })
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
            return if val.len() > MAX_VARIANT_SIZE {
                Err(ParseTypeError::SizeExceeded)
            } else {
                Self::new(typ, val)
            };
        }
        Err(ParseTypeError::Encoding)
    }
}

impl fmt::Display for TraitValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Str(v) => write!(f, "str::{v}"),
            Self::Int(v) => write!(f, "int::{v}"),
            Self::Float(v) => write!(f, "float::{v}"),
            Self::Bool(v) => write!(f, "bool::{v}"),
        }
    }
}

impl TraitValue {
    /// Infers the type from the raw string and returns the appropriate variant.
    /// Detection order: bool → i32 → f32 → Str.
    pub fn build(value: &str) -> Self {
        if let Ok(b) = value.parse::<bool>() {
            return Self::Bool(b);
        }
        if let Ok(i) = value.parse::<i32>() {
            return Self::Int(i);
        }
        if let Ok(f) = value.parse::<f32>() {
            return Self::Float(f);
        }
        Self::Str(value.to_owned())
    }
}

impl FromStr for TraitValue {
    type Err = ParseTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (typ, val) = s.split_once("::").ok_or(ParseTypeError::Encoding)?;
        match typ {
            "str" => Ok(Self::Str(val.to_owned())),
            "int" => val
                .parse::<i32>()
                .map(Self::Int)
                .map_err(|_| ParseTypeError::Encoding),
            "float" => val
                .parse::<f32>()
                .map(Self::Float)
                .map_err(|_| ParseTypeError::Encoding),
            "bool" => val
                .parse::<bool>()
                .map(Self::Bool)
                .map_err(|_| ParseTypeError::Encoding),
            _ => Err(ParseTypeError::Type(typ.to_owned())),
        }
    }
}

impl sqlx::Type<Sqlite> for TraitValue {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl Encode<'_, Sqlite> for TraitValue {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        Encode::<Sqlite>::encode(self.to_string(), buf)
    }
}

impl<'r> Decode<'r, Sqlite> for TraitValue {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        Self::from_str(s).map_err(Into::into)
    }
}
