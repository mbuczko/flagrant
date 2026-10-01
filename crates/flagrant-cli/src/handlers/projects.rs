use anyhow::bail;
use flagrant_client::http::HttpTransport;
use flagrant_types::{
    Environment, Project,
    payload::{NewProjectPayload, ProjectCreatedResponse},
};

pub fn list_projects(transport: &HttpTransport) -> anyhow::Result<Vec<Project>> {
    match transport.get::<Vec<Project>>("/projects/".into()) {
        Ok(projects) => Ok(projects),
        Err(err) => bail!("Could not list projects: {err}"),
    }
}

pub fn create_with_env(
    name: &str,
    transport: &HttpTransport,
) -> anyhow::Result<(Project, Environment)> {
    match transport.post::<_, ProjectCreatedResponse>(
        "/projects/".into(),
        NewProjectPayload {
            name: name.to_owned(),
        },
    ) {
        Ok(resp) => Ok((resp.project, resp.environment)),
        Err(err) => bail!("Could not create a project: {err}"),
    }
}
