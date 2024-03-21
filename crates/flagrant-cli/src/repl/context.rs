use std::{rc::Rc, sync::RwLock};

use flagrant_client::blocking::HttpClient;
use flagrant_types::{Environment, Project};

pub type ReplContext = Rc<RwLock<HttpClientContext>>;

#[derive(Debug)]
pub struct HttpClientContext {
    pub client: HttpClient,
    pub project: Project,
    pub environment: Option<Environment>,
}

impl HttpClientContext {
    /// Creates a struct shared among all the commands.
    /// Context contains a project information, environment (not set up
    /// initially, may be switched at any time) and HTTP client configured
    /// to communicate with API server.
    ///
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
