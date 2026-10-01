use flagrant_proto::v1::{
    GetFeaturesRequest, feature_resolver_client::FeatureResolverClient, variant_value::Kind,
};
use flagrant_types::{FeatureResponse, VariantValue};
use tonic::transport::Channel;

use crate::transport::AsyncTransport;

pub struct GrpcTransport {
    client: FeatureResolverClient<Channel>,
    bearer_token: Option<String>,
}

impl GrpcTransport {
    pub async fn connect(
        endpoint: impl Into<String>,
        bearer_token: Option<String>,
    ) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(endpoint.into())?.connect().await?;

        Ok(Self {
            client: FeatureResolverClient::new(channel),
            bearer_token,
        })
    }
}

impl AsyncTransport for GrpcTransport {
    async fn get_features(
        &self,
        project: &str,
        environment: &str,
        identity: &str,
    ) -> anyhow::Result<Vec<FeatureResponse>> {
        let mut request = tonic::Request::new(GetFeaturesRequest {
            project: project.to_string(),
            environment: environment.to_string(),
        });

        request
            .metadata_mut()
            .insert("x-flagrant-identity", identity.parse()?);

        if let Some(token) = &self.bearer_token {
            request
                .metadata_mut()
                .insert("authorization", format!("Bearer {token}").parse()?);
        }

        let mut client = self.client.clone();
        let response = client.get_features(request).await?;

        Ok(response
            .into_inner()
            .features
            .into_iter()
            .map(|f| {
                let value = match f.value.and_then(|v| v.kind) {
                    Some(Kind::Text(v)) => VariantValue::Text(v),
                    Some(Kind::Json(v)) => VariantValue::Json(v),
                    Some(Kind::Toml(v)) => VariantValue::Toml(v),
                    None => VariantValue::Text(String::new()),
                };

                FeatureResponse {
                    feature_id: f.feature_id,
                    name: f.name,
                    value,
                    is_enabled: f.is_enabled.unwrap_or(false),
                    is_srv: f.is_srv.unwrap_or(false),
                }
            })
            .collect())
    }
}
