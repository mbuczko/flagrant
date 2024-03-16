use axum::{
    routing::{get, post, put},
    Router,
};
use tower_http::compression::CompressionLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::handlers::projects;
use crate::handlers::{environments, features, variants};

mod errors;
mod extractors;
mod handlers;

pub fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // axum logs rejections from built-in extractors with the `axum::rejection`
                // target, at `TRACE` level. `axum::rejection=trace` enables showing those events
                "flagrant_api=debug,flagrant=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

pub async fn start_api_server() -> anyhow::Result<()> {
    let pool = flagrant::init().await?;
    let app = Router::new()
        .route("/projects/:project_id", get(projects::fetch))
        .route("/projects/:project_id/envs", get(environments::list))
        .route("/projects/:project_id/envs", post(environments::create))
        .route(
            "/projects/:project_id/envs/:env_name",
            get(environments::fetch),
        )
        .route("/projects/:project_id/features", get(features::list))
        .route("/projects/:project_id/features", post(features::create))
        .route(
            "/projects/:project_id/features/:feature_name",
            get(features::fetch),
        )
        .route(
            "/projects/:project_id/features/:feature_name",
            put(features::update),
        )
        .route(
            "/projects/:project_id/features/:feature_name/:env_name/variants",
            get(variants::list),
        )
        .route(
            "/projects/:project_id/features/:feature_name/:env_name/variants",
            post(variants::create),
        )
        .with_state(pool)
        .layer(CompressionLayer::new());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3030")
        .await
        .unwrap();

    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
