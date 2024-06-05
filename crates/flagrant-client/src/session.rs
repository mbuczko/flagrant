use std::sync::RwLock;

use anyhow::bail;
use flagrant_types::{Environment, Project, FeatureValue};

use crate::{http::HttpClient, resource::BaseResource};

#[derive(Debug)]
pub struct Session {
    pub client: HttpClient,
    pub project: RwLock<Project>,
    pub environment: RwLock<Environment>,
}

impl Session {

    #[cfg(feature = "blocking")]
    pub fn init(api_host: String, project_id: u16, environment_id: u16) -> anyhow::Result<Session> {
        let client = HttpClient::new(api_host);
        let path = format!("/projects/{project_id}");

        Self::build(
            client.get::<Project>(path.clone()).ok(),
            client.get::<Environment>(format!("{path}/envs/{environment_id}")).ok(),
            client
        )
    }

    #[cfg(not(feature = "blocking"))]
    pub async fn init(api_host: String, project_id: u16, environment_id: u16) -> anyhow::Result<Session> {
        let client = HttpClient::new(api_host);
        let path = format!("/projects/{project_id}");

        Self::build(
            client.get::<Project>(path.clone()).await.ok(),
            client.get::<Environment>(format!("{path}/envs/{environment_id}")).await.ok(),
            client,
        )
    }

    fn build(project: Option<Project>, environment: Option<Environment>, client: HttpClient) -> anyhow::Result<Session> {
        match (project, environment) {
            (Some(project), Some(environment)) => {
                Ok(Session {
                    client,
                    project: RwLock::new(project),
                    environment: RwLock::new(environment),
                })
            },
            (Some(_), None) => bail!("No environment of given id found."),
            (None, Some(_)) => bail!("No project of given id found."),
            _ => bail!("Neither project nor environment was found."),
        }
    }

    pub fn _set_project(&self, new_project: Project) {
        let mut guard = self.project.write().unwrap();

        std::mem::take(&mut *guard);
        *guard = new_project;
    }

    pub fn set_environment(&self, new_environment: Environment) {
        let mut guard = self.environment.write().unwrap();

        std::mem::take(&mut *guard);
        *guard = new_environment;
    }

    #[cfg(feature = "blocking")]
    pub fn get_feature(&self, ident: &str, name: &'static str) -> Option<FeatureValue> {
        let path = self.environment.as_base_resource().subpath(format!("/ident/{ident}/features/{name}"));
        self.client.get(format!("/api/v1{path}")).ok()
    }

}

pub trait Resource {
    fn as_base_resource(&self) -> BaseResource;
}

impl Resource for RwLock<Project> {
    fn as_base_resource(&self) -> BaseResource {
        BaseResource::Project(self.read().unwrap().id)
    }
}

impl Resource for RwLock<Environment> {
    fn as_base_resource(&self) -> BaseResource {
        BaseResource::Environment(self.read().unwrap().id)
    }
}
