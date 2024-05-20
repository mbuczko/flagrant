use axum::{routing::{delete, get, post, put}, Router
};
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::handlers::projects;
use crate::handlers::{environments, features, variants};

mod api;
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
        // projects
        .route("/projects/:project_id", get(projects::fetch))

        // environments
        .route("/projects/:project_id/envs", get(environments::list))
        .route("/projects/:project_id/envs", post(environments::create))
        .route("/projects/:project_id/envs/:env_id", get(environments::fetch))

        // features
        .route("/envs/:environment_id/features", get(features::list))
        .route("/envs/:environment_id/features", post(features::create))
        .route("/envs/:environment_id/features/:feature_id", get(features::fetch))
        .route("/envs/:environment_id/features/:feature_id", put(features::update))
        .route("/envs/:environment_id/features/:feature_id", delete(features::delete))

        // variants
        .route("/envs/:environment_id/features/:feature_id/variants", get(variants::list))
        .route("/envs/:environment_id/features/:feature_id/variants", post(variants::create))
        .route("/envs/:environment_id/variants/:variant_id", get(variants::fetch))
        .route("/envs/:environment_id/variants/:variant_id", put(variants::update))
        .route("/envs/:environment_id/variants/:variant_id", delete(variants::delete))

        // public API
        .nest("/api/v1",
              Router::new()
                .route("/envs/:environment_id/ident/:ident/features/:feature_name", get(api::get_feature)))

        .with_state(pool)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());


    let listener = tokio::net::TcpListener::bind("127.0.0.1:3030")
        .await
        .unwrap();

    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
