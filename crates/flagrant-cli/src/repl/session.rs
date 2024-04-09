use std::cell::RefCell;

use anyhow::bail;
use flagrant_client::{blocking::HttpClient, resource::BaseResource};
use flagrant_types::{Environment, Project};

pub type ReplSession = RefCell<Session>;

#[derive(Debug)]
pub struct Session {
    pub client: HttpClient,
    pub project: Project,
    pub environment: Environment,
}

impl Session {
    /// Creates a struct shared among all the commands.
    /// Context contains a project/environment information and
    /// HTTP client configured to communicate with API server.
    ///
    /// Returns Error in case of any problems with fetching project data.
    pub fn init(client: HttpClient, project_id: u16, environment_id: u16) -> anyhow::Result<Session> {
        let base_path = format!("/projects/{project_id}");
        if let Ok(project ) = client.get::<Project>(base_path.clone()) {
            if let Ok(environment) = client.get::<Environment>(format!("{base_path}/envs/{environment_id}")) {
                return Ok(Session {
                    client,
                    project,
                    environment,
                })
            }
            bail!("No environment of given id found.")
        }
        bail!("No project of given id found.")
    }

    pub fn switch_environment(&mut self, new_environment: Environment) {
        self.environment = new_environment;
    }
}

pub trait Resource {
    fn as_base_resource(&self) -> BaseResource;
}

impl Resource for Project {
    fn as_base_resource(&self) -> BaseResource {
        BaseResource::Project(self.id)
    }
}

impl Resource for Environment {
    fn as_base_resource(&self) -> BaseResource {
        BaseResource::Environment(self.id)
    }
}
