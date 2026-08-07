use axum::{Json, extract::Path};
use flagrant::models::{commit, environment, project};
use flagrant_types::payload::{CommitPayload, CommitResult};

use crate::{errors::ServiceError, extractors::DbConnection};

/// Applies a single `COMMIT` as one atomic, server-side operation.
///
/// Whichever of `feature`/`identity`/`segment` are present in the payload are applied
/// together within a single transaction, and every `(feature, environment)` pair affected
/// by the combined change (directly, or via a segment/identity override touching it) gets
/// exactly one new snapshot. See [`CommitPayload`] for the request shape.
#[utoipa::path(
    post,
    path = "/projects/{project}/envs/{environment}/commit",
    params(
        ("project" = String, Path, description = "Project name"),
        ("environment" = String, Path, description = "Environment name")
    ),
    request_body = CommitPayload,
    responses(
        (status = 200, description = "Commit applied atomically", body = CommitResult)
    ),
    tag = "commit"
)]
pub async fn apply(
    DbConnection(mut conn): DbConnection,
    Path((project_name, env_name)): Path<(String, String)>,
    Json(payload): Json<CommitPayload>,
) -> Result<Json<CommitResult>, ServiceError> {
    let project = project::get_by_name(&mut conn, project_name).await?;
    let env = environment::get_by_name(&mut conn, &project, env_name).await?;

    let result = commit::apply(&mut conn, &project, &env, payload).await?;
    Ok(Json(result))
}
