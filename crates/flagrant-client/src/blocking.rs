use flagrant_types::{HttpRequestPayload, Project};
use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug)]
pub struct HttpClient {
    api_host: String,
    project_id: u16,
    client: reqwest::blocking::Client,
}

impl HttpClient {
    pub fn new(api_host: String, project_id: u16) -> HttpClient {
        HttpClient {
            api_host,
            project_id,
            client: reqwest::blocking::Client::new(),
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
    pub fn put<S: AsRef<str>, P: HttpRequestPayload + Serialize>(
        &self,
        path: S,
        payload: P,
    ) -> anyhow::Result<()> {
        self
            .client
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

    pub fn post<S: AsRef<str>, T: DeserializeOwned, P: HttpRequestPayload + Serialize>(
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

    pub fn project(&self) -> anyhow::Result<Project> {
        self.get::<_, Project>("")
    }
}
