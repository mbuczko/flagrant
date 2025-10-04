use axum::{
    Router,
    routing::{delete, get, post, put},
};
use sqlx::{Pool, Sqlite};

use crate::handlers::{environments, features, projects, variants};
use crate::{api, handlers::tags};

pub fn init_router() -> Router<Pool<Sqlite>> {
    Router::new()
        // projects
        .route("/projects/:project_id", get(projects::fetch))
        // environments
        .route("/projects/:project_id/envs", get(environments::list))
        .route("/projects/:project_id/envs", post(environments::create))
        .route(
            "/projects/:project_id/envs/name/:env_name",
            get(environments::fetch_by_name),
        )
        .route(
            "/projects/:project_id/envs/:env_id",
            get(environments::fetch_by_id),
        )
        // tags
        .route("/envs/:environment_id/tags", get(tags::list))
        // features
        .route("/envs/:environment_id/features", get(features::list))
        .route("/envs/:environment_id/features", post(features::create))
        .route(
            "/envs/:environment_id/features/name/:feature_name",
            get(features::fetch_by_name),
        )
        .route(
            "/envs/:environment_id/features/:feature_id",
            get(features::fetch_by_id),
        )
        .route(
            "/envs/:environment_id/features/:feature_id",
            put(features::update),
        )
        .route(
            "/envs/:environment_id/features/:feature_id",
            delete(features::delete),
        )
        // variants
        .route(
            "/envs/:environment_id/features/:feature_id/variants",
            get(variants::list),
        )
        .route(
            "/envs/:environment_id/features/:feature_id/variants",
            post(variants::create),
        )
        .route(
            "/envs/:environment_id/variants/:variant_id",
            get(variants::fetch),
        )
        .route(
            "/envs/:environment_id/variants/:variant_id",
            put(variants::update),
        )
        .route(
            "/envs/:environment_id/variants/:variant_id",
            delete(variants::delete),
        )
        // public API
        .nest(
            "/api/v1",
            Router::new().route("/envs/:environment_id/features", get(api::get_features)),
        )
}
