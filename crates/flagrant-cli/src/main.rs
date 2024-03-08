use axum::Router;
use flagrant::models;
use sqlx::{Pool, Sqlite};
use tower_http::compression::CompressionLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod cookie;
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
    initialize(&pool).await?;


    let app = Router::new()
    // .route("/@me", get(users::user_identity))
    // .route_layer(middleware::from_fn(middlewares::add_claim_details))
        .with_state(pool)
        .layer(CompressionLayer::new());


    let listener = tokio::net::TcpListener::bind("127.0.0.1:3030")
        .await
        .unwrap();

    // how to set/read a cookie?
    // https://github.com/tokio-rs/axum/discussions/351
    // https://github.com/tokio-rs/axum/blob/b6b203b3065e4005bda01efac8429176da055ae2/examples/oauth/src/main.rs#L237

    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn initialize(pool: &Pool<Sqlite>) -> anyhow::Result<()> {
    let project = models::project::create_project(pool, "flagrant".into()).await?;
    let environment = models::environment::create_environment(pool, &project, "production".into(), None).await?;
    Ok(())
}
