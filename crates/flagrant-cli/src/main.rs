use axum::Router;
use flagrant::models::{environment, project};
use sqlx::{Pool, Sqlite};
use tower_http::compression::CompressionLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::repl::context::ReplContext;

mod repl;
mod errors;
mod extractors;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // axum logs rejections from built-in extractors with the `axum::rejection`
                // target, at `TRACE` level. `axum::rejection=trace` enables showing those events
                "flagrant_cli=debug,flagrant=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let pool = flagrant::init().await?;
    let proj = initialize_project(&pool).await?;


    // let app = Router::new()
    //     .route("/@me", get(users::user_identity))
    //     .with_state(pool)
    //     .layer(CompressionLayer::new());


    let listener = tokio::net::TcpListener::bind("127.0.0.1:3030")
        .await
        .unwrap();

    tracing::info!("listening on {}", listener.local_addr().unwrap());
    // axum::serve(listener, app).await.unwrap();

    repl::readline::init(ReplContext::builder(proj, pool).build())?;

    Ok(())
}

/// Initialize temporary project with sample environment
async fn initialize_project(pool: &Pool<Sqlite>) -> anyhow::Result<project::Project> {
    let project = project::create_project(pool, "flagrant".into()).await?;
    let _env = environment::create_environment(pool, &project, "production", None).await?;
    let _env = environment::create_environment(pool, &project, "development", None).await?;
    let _env = environment::create_environment(pool, &project, "beta", None).await?;

    Ok(project)
}
