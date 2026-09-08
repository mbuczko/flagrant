use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VersionResponse {
    version: &'static str,
}

/// Returns the running server's crate version. Doesn't touch the database or any other
/// dependency, so it also doubles as a cheap k8s liveness/readiness check target.
#[utoipa::path(
    get,
    path = "/version",
    responses(
        (status = 200, description = "Server version", body = VersionResponse),
    ),
    tag = "admin"
)]
pub async fn get_version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION"),
    })
}
