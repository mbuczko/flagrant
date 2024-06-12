use reqwest::Response;
use serde::{de::DeserializeOwned, Serialize};

use crate::http::HttpClient;

impl HttpClient {
    pub fn new(host: String) -> HttpClient {
        let client = reqwest::Client::new();
        HttpClient::Async(client, host)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: String) -> anyhow::Result<T> {
        match self {
            HttpClient::Async(client, host) => {
                match client.get(format!("{host}{path}")).send().await {
                    Ok(response) if response.status().is_success() => Ok(response.json::<T>().await?),
                    Ok(response) => Err(anyhow::anyhow!(response.text().await?)),
                    Err(err) => Err(err.into()),
                }
            },
            _ => unimplemented!()
        }
    }

    pub async fn put<P: Serialize>(&self, path: String, payload: P) -> anyhow::Result<()> {
        match self {
            HttpClient::Async(client, host) => {
                let result = client
                    .put(format!("{host}{path}"))
                    .json(&payload)
                    .send()
                    .await;

                match result {
                    Ok(response) if response.status().is_success() => Ok(()),
                    Ok(response) => Err(anyhow::anyhow!(response.text().await?)),
                    Err(err) => Err(err.into()),
                }
            },
            _ => unimplemented!()
        }
    }

    pub async fn post<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        payload: P,
    ) -> anyhow::Result<T> {
        match self {
            HttpClient::Async(client, host) => {
                let result = client
                    .post(format!("{host}{path}"))
                    .json(&payload)
                    .send()
                    .await;

                match result {
                    Ok(response) if response.status().is_success() => Ok(response.json::<T>().await?),
                    Ok(response) => Err(anyhow::anyhow!(response.text().await?)),
                    Err(err) => Err(err.into()),
                }
            },
            _ => unimplemented!()
        }
    }

    pub async fn delete(&self, path: String) -> anyhow::Result<Response> {
        match self {
            HttpClient::Async(client, host) => {
                let result = client
                    .delete(format!("{host}{path}"))
                    .send()
                    .await;

                match result {
                    Ok(response) if response.status().is_success() => Ok(response),
                    Ok(response) => Err(anyhow::anyhow!(response.text().await?)),
                    Err(err) => Err(err.into()),
                }
            },
            _ => unimplemented!()
        }
    }
}

