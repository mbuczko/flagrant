use std::cell::RefCell;

use anyhow::bail;
use flagrant_client::blocking::HttpClient;
use flagrant_types::{Environment, Project};

pub type ReplContext = RefCell<HttpClientContext>;

#[derive(Debug)]
pub struct HttpClientContext {
    pub client: HttpClient,
    pub project: Project,
    pub environment: Environment,
}

impl HttpClientContext {
    /// Creates a struct shared among all the commands.
    /// Context contains a project/environment information and
    /// HTTP client configured to communicate with API server.
    ///
    /// Returns Error in case of any problems with fetching project data.
    pub fn new(client: HttpClient) -> anyhow::Result<HttpClientContext> {
        if let Ok(project ) = client.project() {
            if let Ok(environment) = client.environment() {
                return Ok(HttpClientContext {
                    client,
                    project,
                    environment,
                })
            }
            bail!("No such an environment found.")
        }
        bail!("No project of given id found.")
    }
}
