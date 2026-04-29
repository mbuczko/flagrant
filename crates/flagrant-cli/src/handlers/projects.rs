use anyhow::bail;
use flagrant_client::http::HttpClient;
use flagrant_types::{Environment, Project, payload::ProjectRequestPayload};

pub fn create_project(name: &str, client: &HttpClient) -> anyhow::Result<(Project, Environment)> {
    match client.post::<_, (Project, Environment)>(
        "/projects/".into(),
        ProjectRequestPayload {
            name: name.to_owned(),
        },
    ) {
        Ok((project, env)) => Ok((project, env)),
        Err(err) => bail!("Could not create a project: {err}"),
    }
}

pub fn create_with_env(name: &str, client: &HttpClient) -> anyhow::Result<(Project, Environment)> {
    let (project, env) = create_project(name, client)?;
    Ok((project, env))
}
