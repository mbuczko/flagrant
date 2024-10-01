use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tracing::init_tracing;

mod api;
mod errors;
mod extractors;
mod handlers;
mod routes;
mod tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let pool = flagrant::db::init_pool()
        .await
        .expect("Cannot initialize DB");
    let router = routes::init_router()
        .with_state(pool)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3030")
        .await
        .expect("Cannot listen on port 3030");

    ::tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, router)
        .await
        .expect("Cannot start HTTP server");
}
