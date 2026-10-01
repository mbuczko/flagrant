use flagrant_types::FeatureResponse;

use crate::transport::AsyncTransport;

pub struct HttpAsyncTransport {
    client: reqwest::Client,
    host: String,
    bearer_token: Option<String>,
}

impl HttpAsyncTransport {
    pub fn new(host: String, bearer_token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            host,
            bearer_token,
        }
    }
}

impl AsyncTransport for HttpAsyncTransport {
    async fn get_features(
        &self,
        project: &str,
        environment: &str,
        identity: &str,
    ) -> anyhow::Result<Vec<FeatureResponse>> {
        let path = format!(
            "{}/api/v1/projects/{}/envs/{}/features",
            self.host, project, environment
        );
        let mut req = self
            .client
            .get(path)
            .header("X-Flagrant-Identity", identity);

        if let Some(token) = &self.bearer_token {
            req = req.bearer_auth(token);
        }

        match req.send().await {
            Ok(response) if response.status().is_success() => Ok(response.json().await?),
            Ok(response) => Err(anyhow::anyhow!(response.text().await?)),
            Err(err) => Err(err.into()),
        }
    }
}
