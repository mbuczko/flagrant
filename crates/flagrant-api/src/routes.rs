use axum::{
    Router,
    routing::{delete, get, patch, post, put},
};
use sqlx::{Pool, Sqlite};
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::handlers::{environments, features, identities, projects, traits, variants};
use crate::openapi::ApiDoc;
use crate::{api, handlers::tags};

pub fn init_router() -> Router<Pool<Sqlite>> {
    let project_routes = Router::new()
        // Environments
        .route("/envs", get(environments::list))
        .route("/envs", post(environments::create))
        .route("/envs/:env_id", get(environments::fetch_by_id_or_name))
        // Tags
        .route("/envs/:environment/tags", get(tags::list))
        // Features
        .route("/envs/:environment/features", get(features::list))
        .route("/envs/:environment/features", post(features::create))
        .route(
            "/envs/:environment/features/:feature_id",
            get(features::fetch_by_id_or_name),
        )
        .route(
            "/envs/:environment/features/:feature_id",
            put(features::update),
        )
        .route(
            "/envs/:environment/features/:feature_id",
            delete(features::delete),
        )
        .route(
            "/envs/:environment/features/:feature_id",
            patch(features::patch),
        )
        // Variants
        .route(
            "/envs/:environment/features/:feature_id/variants",
            get(variants::list),
        )
        .route(
            "/envs/:environment/features/:feature_id/variants",
            post(variants::create),
        )
        .route(
            "/envs/:environment/variants/:variant_id",
            get(variants::fetch),
        )
        .route(
            "/envs/:environment/variants/:variant_id",
            put(variants::update),
        )
        .route(
            "/envs/:environment/variants/:variant_id",
            delete(variants::delete),
        )
        // Identities
        .route("/envs/:environment/identities", get(identities::list))
        .route("/envs/:environment/identities", post(identities::create))
        .route("/envs/:environment/identities/:identity", get(identities::fetch))
        .route("/envs/:environment/identities/:identity", delete(identities::delete))
        .route(
            "/envs/:environment/identities/:identity",
            patch(identities::update),
        )
        .route(
            "/envs/:environment/identities/:identity/variants",
            get(identities::get_variants),
        )
        // Traits
        .route("/traits", get(traits::list))
        .route("/traits", post(traits::create))
        .route("/traits/:trait_id", delete(traits::delete));

    Router::new()
        .merge(Scalar::with_url("/scalar", ApiDoc::openapi()))
        // Projects
        .route("/projects/", get(projects::list))
        .route("/projects/", post(projects::create))
        .route("/projects/:project", get(projects::fetch))
        .nest("/projects/:project", project_routes)
        // Public API
        .nest(
            "/api/v1/projects/:project",
            Router::new().route("/envs/:environment/features", get(api::get_features)),
        )
}
