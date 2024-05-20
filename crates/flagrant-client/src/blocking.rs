use reqwest::blocking::Response;
use serde::{de::DeserializeOwned, Serialize};

#[derive(Debug)]
pub struct HttpClient {
    api_host: String,
    client: reqwest::blocking::Client,
}

impl HttpClient {
    pub fn new(api_host: String) -> HttpClient {
        let client = reqwest::blocking::Client::new();
        HttpClient { api_host, client }
    }

    pub fn get<T: DeserializeOwned>(&self, path: String) -> anyhow::Result<T> {
        let result = reqwest::blocking::get(format!("{}{}", self.api_host, path));

        match result {
            Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }

    pub fn put<P: Serialize>(&self, path: String, payload: P) -> anyhow::Result<()> {
        let result = self
            .client
            .put(format!("{}{}", self.api_host, path))
            .json(&payload)
            .send();

        match result {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }

    pub fn post<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        payload: P,
    ) -> anyhow::Result<T> {
        let result = self
            .client
            .post(format!("{}{}", self.api_host, path))
            .json(&payload)
            .send();

        match result {
            Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }

    pub fn delete(&self, path: String) -> anyhow::Result<Response> {
        let result = self
            .client
            .delete(format!("{}{}", self.api_host, path))
            .send();

        match result {
            Ok(response) if response.status().is_success() => Ok(response),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }
}
