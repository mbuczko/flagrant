use flagrant_types::{Environment, Project};
use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug)]
pub struct HttpClient {
    api_host: String,
    project_id: u16,
    env_name: String,
    client: reqwest::blocking::Client,
}

impl HttpClient {
    pub fn new(api_host: String, project_id: u16, env_name: String) -> HttpClient {
        HttpClient {
            client: reqwest::blocking::Client::new(),
            api_host,
            project_id,
            env_name,
        }
    }

    pub fn get<S: AsRef<str>, T: DeserializeOwned>(&self, path: S) -> anyhow::Result<T> {
        Ok(reqwest::blocking::get(format!(
            "{}/projects/{}{}",
            self.api_host,
            self.project_id,
            path.as_ref()
        ))?
        .json::<T>()?)
    }
    pub fn put<S: AsRef<str>, P: Serialize>(
        &self,
        path: S,
        payload: P,
    ) -> anyhow::Result<()> {
        self.client
            .put(format!(
                "{}/projects/{}{}",
                self.api_host,
                self.project_id,
                path.as_ref()
            ))
            .json(&payload)
            .send()?;

        Ok(())
    }

    pub fn post<S: AsRef<str>, P: Serialize, T: DeserializeOwned>(
        &self,
        path: S,
        payload: P,
    ) -> anyhow::Result<T> {
        Ok(self
            .client
            .post(format!(
                "{}/projects/{}{}",
                self.api_host,
                self.project_id,
                path.as_ref()
            ))
            .json(&payload)
            .send()?
            .json::<T>()?)
    }

    pub fn delete<S: AsRef<str>>(&self, path: S) -> anyhow::Result<()> {
        self.client
            .delete(format!(
                "{}/projects/{}{}",
                self.api_host,
                self.project_id,
                path.as_ref()
            ))
            .send()?;

        Ok(())
    }

    pub fn project(&self) -> anyhow::Result<Project> {
        self.get::<_, Project>("")
    }
    pub fn environment(&self) -> anyhow::Result<Environment> {
        self.get::<_, Environment>(format!("/envs/{}", self.env_name))
    }
}
