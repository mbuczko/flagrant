pub mod blocking;

use flagrant_types::Project;
use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug)]
pub struct HttpClient {
    api_host: String,
    project_id: u16,
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new(api_host: String, project_id: u16) -> HttpClient {
        HttpClient {
            api_host,
            project_id,
            client: reqwest::Client::new(),
        }
    }

    pub async fn get<S: AsRef<str>, T: DeserializeOwned>(&self, path: S) -> anyhow::Result<T> {
        Ok(reqwest::get(format!(
            "{}/projects/{}{}",
            self.api_host,
            self.project_id,
            path.as_ref()
        ))
        .await?
        .json::<T>()
        .await?)
    }

    pub async fn put<S: AsRef<str>, P: Serialize>(
        &self,
        path: S,
        payload: &P,
    ) -> anyhow::Result<()> {
        self.client
            .put(format!(
                "{}/projects/{}{}",
                self.api_host,
                self.project_id,
                path.as_ref()
            ))
            .json(payload)
            .send()
            .await?;

        Ok(())
    }

    pub async fn post<S: AsRef<str>, P: Serialize, T: DeserializeOwned>(
        &self,
        path: S,
        payload: &P,
    ) -> anyhow::Result<T> {
        Ok(self
            .client
            .post(format!(
                "{}/projects/{}{}",
                self.api_host,
                self.project_id,
                path.as_ref()
            ))
            .json(payload)
            .send()
            .await?
            .json::<T>()
            .await?)
    }

    pub async fn project(&self) -> anyhow::Result<Project> {
        self.get::<_, Project>("").await
    }
}
