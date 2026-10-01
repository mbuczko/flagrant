use flagrant_types::FeatureResponse;

use crate::transport::{AsyncTransport, BlockingTransport};

/// Pairs a synchronous transport with the one capability it's allowed to expose to external
/// consumers. No matter which transport backs it (HTTP blocking, or a future caching wrapper
/// around one), only feature resolution for an identity is reachable here. Pinned to a single
/// project for its lifetime - same reasoning as `Connection` pinning project+environment in
/// `flagrant-client` - but environment is taken per call, since one client/project commonly
/// spans multiple environments (dev/prod).
pub struct FlagrantClient<T: BlockingTransport> {
    transport: T,
    project: String,
}

impl<T: BlockingTransport> FlagrantClient<T> {
    pub fn new(transport: T, project: String) -> Self {
        Self { transport, project }
    }

    pub fn get_features(
        &self,
        environment: &str,
        identity: &str,
    ) -> anyhow::Result<Vec<FeatureResponse>> {
        self.transport
            .get_features(&self.project, environment, identity)
    }
}

/// Async counterpart of [`FlagrantClient`], for async-native transports (HTTP async, gRPC).
pub struct AsyncFlagrantClient<T: AsyncTransport> {
    transport: T,
    project: String,
}

impl<T: AsyncTransport> AsyncFlagrantClient<T> {
    pub fn new(transport: T, project: String) -> Self {
        Self { transport, project }
    }

    pub async fn get_features(
        &self,
        environment: &str,
        identity: &str,
    ) -> anyhow::Result<Vec<FeatureResponse>> {
        self.transport
            .get_features(&self.project, environment, identity)
            .await
    }
}
