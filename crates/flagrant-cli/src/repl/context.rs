use std::sync::{Arc, Mutex};

use flagrant_client::blocking::HttpClient;
use flagrant_types::{Environment, Project};

pub type ReplContext = Arc<Mutex<HttpClientContext>>;

#[derive(Debug)]
pub struct HttpClientContext {
    pub client: HttpClient,
    pub project: Project,
    pub environment: Option<Environment>,
}

impl HttpClientContext {
    /// Creates a struct shared among all the commands.
    /// Context contains a project information and HTTP client used
    /// to communicate with API server.
    /// Returns Error in case of any problems with fetching project data.
    pub fn new(client: HttpClient) -> anyhow::Result<HttpClientContext> {
        let project = client.project()?;
        Ok(HttpClientContext {
            client,
            project,
            environment: None,
        })
    }
}
