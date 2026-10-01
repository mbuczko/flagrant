use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Axum logs rejections from built-in extractors with the `axum::rejection`
                // target, at `TRACE` level. `axum::rejection=trace` enables showing those events.
                "flagrant_api=info,flagrant=info,axum::rejection=debug".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
