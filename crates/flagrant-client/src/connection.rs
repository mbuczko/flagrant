use anyhow::bail;
use flagrant_types::{Environment, Feature, FeatureResponse, Project};

use crate::{
    http::{Auth, HttpClient},
    resource::BaseResource,
};

#[derive(Debug)]
pub struct Connection {
    pub client: HttpClient,
    pub project: Project,
    pub feature: Option<Feature>,
    pub environment: Environment,
}

impl Connection {
    #[cfg(feature = "blocking")]
    pub fn init(
        api_host: String,
        auth: Auth,
        project_id: i32,
        environment_id: i32,
    ) -> anyhow::Result<Connection> {
        let client = HttpClient::new(api_host, auth);
        let path = format!("/projects/{project_id}");

        Self::build(
            client.get::<Project>(path.clone()).ok(),
            client
                .get::<Environment>(format!("{path}/envs/{environment_id}"))
                .ok(),
            client,
        )
    }

    #[cfg(not(feature = "blocking"))]
    pub async fn init(
        api_host: String,
        project_id: i32,
        environment_id: i32,
    ) -> anyhow::Result<Connection> {
        let client = HttpClient::new(api_host, Auth::None);
        let path = format!("/projects/{project_id}");

        Self::build(
            client.get::<Project>(path.clone()).await.ok(),
            client
                .get::<Environment>(format!("{path}/envs/{environment_id}"))
                .await
                .ok(),
            client,
        )
    }

    fn build(
        project: Option<Project>,
        environment: Option<Environment>,
        client: HttpClient,
    ) -> anyhow::Result<Connection> {
        match (project, environment) {
            (Some(project), Some(environment)) => Ok(Connection {
                client,
                project,
                environment,
                feature: None,
            }),
            (Some(_), None) => bail!("No environment of given id found."),
            (None, Some(_)) => bail!("No project of given id found."),
            _ => bail!("Neither project nor environment was found."),
        }
    }

    #[cfg(feature = "blocking")]
    pub fn get_features(&self, identity: &str) -> Option<Vec<FeatureResponse>> {
        let path = self.environment.as_base_resource().subpath("/features");
        self.client
            .get_with_identity(format!("/api/v1{path}"), Some(identity))
            .ok()
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
