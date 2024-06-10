use std::{fmt, str::FromStr};

use sqlx::{encode::IsNull, sqlite::{SqliteArgumentValue, SqliteValueRef}, Decode, Encode, Sqlite, Type};
use thiserror::Error;

use crate::FeatureValue;

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
