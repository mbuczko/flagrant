use reqwest::blocking::Response;
use serde::{de::DeserializeOwned, Serialize};

use crate::http::HttpClient;

impl HttpClient {
    pub fn new(host: String) -> HttpClient {
        let client = reqwest::blocking::Client::new();
        HttpClient::Blocking(client, host)
    }

    pub fn get<T: DeserializeOwned>(&self, path: String) -> anyhow::Result<T> {
        match self {
            HttpClient::Blocking(client, host) => {
                match client.get(format!("{host}{path}")).send() {
                    Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            },
            _ => unimplemented!()
        }
    }

    pub fn put<P: Serialize>(&self, path: String, payload: P) -> anyhow::Result<()> {
        match self {
            HttpClient::Blocking(client, host) => {
                let result = client
                    .put(format!("{host}{path}"))
                    .json(&payload)
                    .send();

                match result {
                    Ok(response) if response.status().is_success() => Ok(()),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            },
            _ => unimplemented!()
        }
    }

    pub fn post<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        payload: P,
    ) -> anyhow::Result<T> {
        match self {
            HttpClient::Blocking(client, host) => {
                let result = client
                    .post(format!("{host}{path}"))
                    .json(&payload)
                    .send();

                match result {
                    Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            },
            _ => unimplemented!()
        }
    }

    pub fn delete(&self, path: String) -> anyhow::Result<Response> {
        match self {
            HttpClient::Blocking(client, host) => {
                let result = client
                    .delete(format!("{host}{path}"))
                    .send();

                match result {
                    Ok(response) if response.status().is_success() => Ok(response),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            },
            _ => unimplemented!()
        }
    }
}
