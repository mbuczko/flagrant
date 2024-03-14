use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, sqlx::FromRow)]
pub struct Project {
    #[sqlx(rename = "project_id")]
    pub id: u16,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, sqlx::FromRow)]
pub struct Environment {
    #[sqlx(rename = "environment_id")]
    pub id: u16,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct NewEnvRequestPayload {
    pub name: String,
    pub description: Option<String>
}

pub trait HttpRequestPayload {}

impl HttpRequestPayload for NewEnvRequestPayload {}
