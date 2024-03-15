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
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct NewEnvRequestPayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct NewFeatureRequestPayload {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Feature {
    #[sqlx(rename = "feature_id")]
    pub id: u16,
    pub project_id: u16,
    pub name: String,
    pub value: String,
    pub is_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, sqlx::FromRow)]
pub struct Variant {
    #[sqlx(rename = "variant_id")]
    pub id: u16,
    pub value: String,
    pub weight: i16,
    pub acc: i16,
}
pub trait HttpRequestPayload {}

impl HttpRequestPayload for NewEnvRequestPayload {}
impl HttpRequestPayload for NewFeatureRequestPayload {}
