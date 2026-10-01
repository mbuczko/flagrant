use flagrant_types::FeatureResponse;

/// Resolves features for an identity in a given environment, over a synchronous transport.
/// Implementors reach the public evaluation endpoint only - there is deliberately no way to
/// perform admin operations (project/feature/segment management etc.) through this trait.
pub trait BlockingTransport {
    fn get_features(
        &self,
        project: &str,
        environment: &str,
        identity: &str,
    ) -> anyhow::Result<Vec<FeatureResponse>>;
}

/// Async counterpart of [`BlockingTransport`], for transports that are async-native (HTTP via
/// `reqwest`, or gRPC via `tonic` which has no synchronous client at all).
#[allow(async_fn_in_trait)]
pub trait AsyncTransport {
    async fn get_features(
        &self,
        project: &str,
        environment: &str,
        identity: &str,
    ) -> anyhow::Result<Vec<FeatureResponse>>;
}
