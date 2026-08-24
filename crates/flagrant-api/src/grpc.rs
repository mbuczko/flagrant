use std::path::Path;

use flagrant::errors::FlagrantError;
use flagrant_types::VariantValue;
use tokio::net::{TcpListener, UnixListener};
use tokio_stream::wrappers::{TcpListenerStream, UnixListenerStream};
use tonic::{Request, Response, Status, transport::Server};

use crate::{api, config::GrpcConfig, state::AppState};

pub mod proto {
    tonic::include_proto!("flagrant.v1");
}

use proto::{
    Feature, GetFeaturesRequest, GetFeaturesResponse,
    feature_resolver_server::{FeatureResolver, FeatureResolverServer},
    variant_value::Kind,
};

pub struct GrpcFeatureResolver {
    state: AppState,
}

#[tonic::async_trait]
impl FeatureResolver for GrpcFeatureResolver {
    async fn get_features(
        &self,
        request: Request<GetFeaturesRequest>,
    ) -> Result<Response<GetFeaturesResponse>, Status> {
        let (metadata, _extensions, req) = request.into_parts();

        let identity = metadata
            .get("x-flagrant-identity")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| Status::unauthenticated("No x-flagrant-identity metadata found"))?;

        let bearer_token = metadata
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        let mut conn = self
            .state
            .pool
            .acquire()
            .await
            .map_err(|e| Status::internal(format!("DB connection failed: {e}")))?;

        let features = api::resolve_features(
            &self.state,
            &mut conn,
            req.project,
            req.environment,
            identity,
            bearer_token,
        )
        .await
        .map_err(map_error)?;

        let features = features
            .into_iter()
            .map(|f| Feature {
                feature_id: f.feature_id,
                name: f.name,
                value: Some(proto::VariantValue {
                    kind: Some(match f.value {
                        VariantValue::Text(v) => Kind::Text(v),
                        VariantValue::Json(v) => Kind::Json(v),
                        VariantValue::Toml(v) => Kind::Toml(v),
                    }),
                }),
                is_enabled: Some(f.is_enabled),
            })
            .collect();

        Ok(Response::new(GetFeaturesResponse { features }))
    }
}

/// Mirrors errors.rs's `ServiceError` -> HTTP status mapping, transposed to gRPC status
/// codes. Internal faults are logged here in full and returned to the caller only as
/// `Status::internal` with a generic message - same "never leak internals" policy as the
/// HTTP side.
fn map_error(err: anyhow::Error) -> Status {
    match err.downcast_ref::<FlagrantError>() {
        Some(FlagrantError::UnexpectedFailure(msg, cause)) => {
            tracing::error!(cause = ?cause, msg);
            Status::internal(*msg)
        }
        Some(FlagrantError::QueryFailed(msg, cause)) => {
            tracing::error!(cause = ?cause, msg);
            Status::internal(*msg)
        }
        Some(FlagrantError::BadRequest(msg)) => Status::invalid_argument(*msg),
        Some(FlagrantError::NoIdentity(msg)) => Status::unauthenticated(*msg),
        Some(FlagrantError::NotFound(msg)) => Status::not_found(*msg),
        Some(FlagrantError::InvalidOperation(msg)) => Status::failed_precondition(*msg),
        Some(err @ FlagrantError::VersionMismatch { .. }) => {
            Status::failed_precondition(err.to_string())
        }
        None => {
            tracing::error!(error = ?err, "Unexpected error");
            Status::internal("Unexpected error")
        }
    }
}

/// Runs the gRPC server to completion (never returns on success) on either a TCP
/// `host:port` or a `unix:<path>` listener, depending on `config.listen`. Intended to be
/// `tokio::spawn`ed as a best-effort background task by `main.rs` - a failure here is
/// logged, not propagated, so it never brings down the HTTP server.
pub async fn serve(config: GrpcConfig, state: AppState) -> anyhow::Result<()> {
    let svc = FeatureResolverServer::new(GrpcFeatureResolver { state });

    if let Some(path) = config.listen.strip_prefix("unix:") {
        // Clean up a stale socket file left behind by a previous, uncleanly-terminated run
        // - UnixListener::bind fails with AddrInUse otherwise even though nothing is
        // actually listening on it.
        if Path::new(path).exists() {
            std::fs::remove_file(path)?;
        }
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(path)?;
        tracing::info!("gRPC listening on unix:{path}");

        Server::builder()
            .add_service(svc)
            .serve_with_incoming(UnixListenerStream::new(listener))
            .await?;
    } else {
        let listener = TcpListener::bind(&config.listen).await?;
        tracing::info!("gRPC listening on {}", listener.local_addr()?);

        Server::builder()
            .add_service(svc)
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await?;
    }

    Ok(())
}
