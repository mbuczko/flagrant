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
        Ok(reqwest::blocking::get(format!("{}{}", self.api_host, path))?.json::<T>()?)
    }

    pub fn put<P: Serialize>(&self, path: String, payload: P) -> anyhow::Result<()> {
        self.client
            .put(format!("{}{}", self.api_host, path))
            .json(&payload)
            .send()?;

        Ok(())
    }

    pub fn post<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        payload: P,
    ) -> anyhow::Result<T> {
        Ok(self
            .client
            .post(format!("{}{}", self.api_host, path))
            .json(&payload)
            .send()?
            .json::<T>()?)
    }

    pub fn delete(&self, path: String) -> anyhow::Result<()> {
        self.client
            .delete(format!("{}{}", self.api_host, path))
            .send()?;

        Ok(())
    }
}
