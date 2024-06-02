use std::cell::RefCell;

use anyhow::bail;
use flagrant_client::{blocking::HttpClient, resource::BaseResource};
use flagrant_types::{Environment, Project};

#[derive(Debug)]
pub struct Session {
    pub client: HttpClient,
    pub project: RefCell<Project>,
    pub environment: RefCell<Environment>,
}

impl Session {
    /// Creates a struct shared among all the commands.
    /// Session contains a project/environment information and HTTP client
    /// configured to communicate with API server.
    ///
    /// Returns Error in case of any problems with getting project or environment
    /// data from API server.
    pub fn init(client: HttpClient, project_id: u16, environment_id: u16) -> anyhow::Result<Session> {
        let base_path = format!("/projects/{project_id}");
        if let Ok(project ) = client.get::<Project>(base_path.clone()) {
            if let Ok(environment) = client.get::<Environment>(format!("{base_path}/envs/{environment_id}")) {
                return Ok(Session {
                    client,
                    project: RefCell::new(project),
                    environment: RefCell::new(environment),
                })
            }
            bail!("No environment of given id found.")
        }
        bail!("No project of given id found.")
    }

    pub fn _switch_project(&self, new_project: Project) {
        self.project.replace_with(move |_| new_project);
    }

    pub fn switch_environment(&self, new_environment: Environment) {
        self.environment.replace_with(move |_| new_environment);
    }
}

pub trait Resource {
    fn as_base_resource(&self) -> BaseResource;
}

impl Resource for RefCell<Project> {
    fn as_base_resource(&self) -> BaseResource {
        BaseResource::Project(self.borrow().id)
    }
}

impl Resource for RefCell<Environment> {
    fn as_base_resource(&self) -> BaseResource {
        BaseResource::Environment(self.borrow().id)
    }
}
