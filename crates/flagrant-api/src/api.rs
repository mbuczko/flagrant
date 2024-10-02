use axum::{
    extract::{Path, State},
    Json,
};
use flagrant::models::{environment, identity};
use flagrant_types::FeatureValue;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::{errors::ServiceError, extractors::Identity};

#[derive(Serialize)]
pub(crate) struct FeatureVariant<'a> {
    id: u16,
    name: String,
    value: String,
    r#type: &'a str,
}

pub async fn get_features<'a>(
    State(pool): State<SqlitePool>,
    Path(environment_id): Path<u16>,
    Identity(identity): Identity,
) -> Result<Json<Vec<FeatureVariant<'a>>>, ServiceError> {
    let mut conn = pool.acquire().await?;
    let env = environment::get_by_id(&mut conn, environment_id).await?;
    let variants = identity::get_variants(&mut conn, &env, identity)
        .await?
        .into_iter()
        .map(|v| {
            let (value, r#type) = match v.value {
                FeatureValue::Text(s) => (s, "text"),
                FeatureValue::Json(s) => (s, "json"),
                FeatureValue::Toml(s) => (s, "toml"),
            };
            FeatureVariant {
                id: v.feature_id,
                name: v.name,
                value,
                r#type,
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(variants))
}
