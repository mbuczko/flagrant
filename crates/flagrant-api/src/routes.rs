use axum::{
    Router,
    routing::{delete, get, patch, post, put},
};
use sqlx::{Pool, Sqlite};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::handlers::{environments, features, identities, projects, traits, variants};
use crate::openapi::ApiDoc;
use crate::{api, handlers::tags};

pub fn init_router() -> Router<Pool<Sqlite>> {
    Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Projects
        .route("/projects/", get(projects::list))
        .route("/projects/:project_id", get(projects::fetch))
        .route("/projects/", post(projects::create))
        // Environments
        .route("/projects/:project_id/envs", get(environments::list))
        .route("/projects/:project_id/envs", post(environments::create))
        .route(
            "/projects/:project_id/envs/:env_id",
            get(environments::fetch_by_id_or_name),
        )
        // Tags
        .route("/envs/:environment_id/tags", get(tags::list))
        // Features
        .route("/envs/:environment_id/features", get(features::list))
        .route("/envs/:environment_id/features", post(features::create))
        .route(
            "/envs/:environment_id/features/:feature_id",
            get(features::fetch_by_id_or_name),
        )
        .route(
            "/envs/:environment_id/features/:feature_id",
            put(features::update),
        )
        .route(
            "/envs/:environment_id/features/:feature_id",
            delete(features::delete),
        )
        .route(
            "/envs/:environment_id/features/:feature_id",
            patch(features::patch),
        )
        // Variants
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
        // Identities
        .route("/identities", get(identities::list))
        .route("/identities", post(identities::create))
        .route("/identities/:identity_id", get(identities::fetch))
        .route("/identities/:identity_id", put(identities::update))
        .route("/identities/:identity_id", delete(identities::delete))
        // Traits
        .route("/traits", get(traits::list))
        .route("/traits", post(traits::create))
        .route("/traits/:trait_id", delete(traits::delete))
        // Public API
        .nest(
            "/api/v1",
            Router::new().route("/envs/:environment_id/features", get(api::get_features)),
        )
}
