use flagrant_types::FeatureResponse;

use crate::transport::BlockingTransport;

pub struct HttpBlockingTransport {
    client: reqwest::blocking::Client,
    host: String,
    bearer_token: Option<String>,
}

impl HttpBlockingTransport {
    pub fn new(host: String, bearer_token: Option<String>) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            host,
            bearer_token,
        }
    }
}

impl BlockingTransport for HttpBlockingTransport {
    fn get_features(
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

        match req.send() {
            Ok(response) if response.status().is_success() => Ok(response.json()?),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }
}
