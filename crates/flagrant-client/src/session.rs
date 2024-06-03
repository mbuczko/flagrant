use std::cell::RefCell;

use anyhow::bail;
use flagrant_types::{Environment, Project};

use crate::{http::HttpClient, resource::BaseResource};

#[derive(Debug)]
pub struct Session {
    pub client: HttpClient,
    pub project: RefCell<Project>,
    pub environment: RefCell<Environment>,
}

impl Session {

    #[cfg(feature = "blocking")]
    pub fn init(api_host: String, project_id: u16, environment_id: u16) -> anyhow::Result<Session> {
        let client = HttpClient::new(api_host);
        let path = format!("/projects/{project_id}");
        if let Ok(project ) = client.get::<Project>(path.clone()) {
            if let Ok(environment) = client.get::<Environment>(format!("{path}/envs/{environment_id}")) {
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

    #[cfg(not(feature = "blocking"))]
    pub async fn init(api_host: String, project_id: u16, environment_id: u16) -> anyhow::Result<Session> {
        let client = HttpClient::new(api_host);
        let path = format!("/projects/{project_id}");
        if let Ok(project ) = client.get::<Project>(path.clone()).await {
            if let Ok(environment) = client.get::<Environment>(format!("{path}/envs/{environment_id}")).await {
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

    // pub fn get_feature(&self, name: String) -> Option<String> {
    //     self.get(format!("/envs/:environment_id/ident/:ident/features/{name}"));
    // }

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
