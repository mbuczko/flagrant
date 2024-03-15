use flagrant_types::{Environment, HttpRequestPayload, Project};
use serde::{de::DeserializeOwned, Serialize};

const API_HOST: &str = "http://localhost:3030";

#[derive(Debug)]
pub struct HttpClient {
    pub project: Project,
    pub environment: Option<Environment>,
    client: reqwest::blocking::Client,
}

impl HttpClient {
    pub fn new(project_id: u16) -> anyhow::Result<HttpClient> {
        let project: Project =
            reqwest::blocking::get(format!("{}/projects/{}", API_HOST, project_id))?
            .json::<Project>()?;

        Ok(HttpClient {
            project,
            environment: None,
            client: reqwest::blocking::Client::new(),
        })
    }

    pub fn get<S: AsRef<str>, T: DeserializeOwned>(&self, path: S) -> anyhow::Result<T> {
        Ok(reqwest::blocking::get(format!("{}{}", API_HOST, path.as_ref()))?.json::<T>()?)
    }

    pub fn post<S: AsRef<str>, T: DeserializeOwned, P: HttpRequestPayload + Serialize>(
        &self,
        path: S,
        payload: &P,
    ) -> anyhow::Result<T> {
        Ok(self
            .client
            .post(format!("{}{}", API_HOST, path.as_ref()))
            .json(payload)
            .send()?
            .json::<T>()?)
    }
}
