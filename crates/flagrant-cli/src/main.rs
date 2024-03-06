use axum::Router;
use tower_http::compression::CompressionLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
    let app = Router::new()
    // .route("/@me", get(users::user_identity))
    // .route_layer(middleware::from_fn(middlewares::add_claim_details))
        .with_state(pool)
        .layer(CompressionLayer::new());


    let listener = tokio::net::TcpListener::bind("127.0.0.1:3030")
        .await
        .unwrap();

    // how to set a cookie?
    // https://github.com/tokio-rs/axum/discussions/351
    //
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
