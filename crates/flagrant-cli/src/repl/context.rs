use flagrant_client::blocking::HttpClient;
use flagrant_types::{Environment, Project};

#[derive(Debug)]
pub struct HttpClientContext {
    pub client: HttpClient,
    pub project: Project,
    pub environment: Option<Environment>
}

impl HttpClientContext {
    pub fn new(client: HttpClient) -> anyhow::Result<HttpClientContext> {
        let project = client.project()?;
        Ok(HttpClientContext { client, project, environment: None })
    }
}
