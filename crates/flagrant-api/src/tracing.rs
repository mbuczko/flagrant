use std::io::IsTerminal;

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
        .with(
            // Colors are only useful when a human is directly watching a real terminal.
            // A container's stdout is a pipe, not a TTY, and some log viewers (e.g. busybox
            // ash) mangle ANSI codes badly enough to eat the rest of the line, so default to
            // plain output whenever stdout isn't a TTY, on top of honoring `NO_COLOR`.
            tracing_subscriber::fmt::layer()
                .with_ansi(std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()),
        )
        .init();
}
