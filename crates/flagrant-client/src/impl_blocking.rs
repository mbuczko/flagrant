use reqwest::blocking::Response;
use serde::{Serialize, de::DeserializeOwned};

use crate::http::{Auth, HttpClient};

impl HttpClient {
    pub fn new(host: String, auth: Auth) -> HttpClient {
        let client = reqwest::blocking::Client::new();
        HttpClient::Blocking(client, host, auth)
    }

    pub(crate) fn get_with_identity<T: DeserializeOwned>(
        &self,
        path: String,
        identity: Option<&str>,
    ) -> anyhow::Result<T> {
        match self {
            HttpClient::Blocking(client, host, _auth) => {
                match client
                    .get(format!("{host}{path}"))
                    .header("X-Flagrant-Identity", identity.unwrap_or_default())
                    .send()
                {
                    Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            }
            _ => unimplemented!(),
        }
    }

    pub(crate) fn post_with_identity<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        _identity: Option<&str>,
        payload: P,
    ) -> anyhow::Result<T> {
        match self {
            HttpClient::Blocking(client, host, _auth) => {
                let result = client.post(format!("{host}{path}")).json(&payload).send();
                match result {
                    Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            }
            _ => unimplemented!(),
        }
    }

    pub fn get<T: DeserializeOwned>(&self, path: String) -> anyhow::Result<T> {
        self.get_with_identity(path, None)
    }

    pub fn post<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        payload: P,
    ) -> anyhow::Result<T> {
        self.post_with_identity(path, None, payload)
    }

    pub fn put<P: Serialize>(&self, path: String, payload: P) -> anyhow::Result<()> {
        match self {
            HttpClient::Blocking(client, host, _auth) => {
                let result = client.put(format!("{host}{path}")).json(&payload).send();

                match result {
                    Ok(response) if response.status().is_success() => Ok(()),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            }
            _ => unimplemented!(),
        }
    }

    pub fn patch<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        payload: P,
    ) -> anyhow::Result<T> {
        match self {
            HttpClient::Blocking(client, host, _auth) => {
                let result = client.patch(format!("{host}{path}")).json(&payload).send();
                match result {
                    Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            }
            _ => unimplemented!(),
        }
    }

    pub fn delete(&self, path: String) -> anyhow::Result<Response> {
        match self {
            HttpClient::Blocking(client, host, _auth) => {
                let result = client.delete(format!("{host}{path}")).send();

                match result {
                    Ok(response) if response.status().is_success() => Ok(response),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            }
            _ => unimplemented!(),
        }
    }
}
