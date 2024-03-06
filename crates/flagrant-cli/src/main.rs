use axum::Router;
use tracing_log::LogTracer;
use tower_http::compression::CompressionLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    LogTracer::init().expect("Failed to set logger");

    let pool = flagrant::init().await?;
    let app = Router::new()
        // .route("/@me", get(users::user_identity))
        // .route_layer(middleware::from_fn(middlewares::add_claim_details))
        .with_state(pool)
        .layer(CompressionLayer::new());


    let listener = tokio::net::TcpListener::bind("127.0.0.1:3030")
        .await
        .unwrap();

    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
