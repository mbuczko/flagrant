use reqwest::blocking::Response;
use serde::{Serialize, de::DeserializeOwned};

use crate::http::{Auth, HttpTransport};

impl HttpTransport {
    pub fn new(host: String, auth: Auth) -> HttpTransport {
        let client = reqwest::blocking::Client::new();
        HttpTransport::Blocking(client, host, auth)
    }

    pub fn get<T: DeserializeOwned>(&self, path: String) -> anyhow::Result<T> {
        match self {
            HttpTransport::Blocking(client, host, _auth) => {
                match client.get(format!("{host}{path}")).send() {
                    Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
                    Ok(response) => Err(anyhow::anyhow!(response.text()?)),
                    Err(err) => Err(err.into()),
                }
            }
            _ => unimplemented!(),
        }
    }

    /// Host this transport talks to - needed to build a `flagrant-sdk` transport for the
    /// public evaluation endpoint, which lives outside this crate's admin-only surface.
    pub fn host(&self) -> &str {
        match self {
            HttpTransport::Blocking(_, host, _) => host,
            _ => unimplemented!(),
        }
    }

    pub fn post<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        payload: P,
    ) -> anyhow::Result<T> {
        match self {
            HttpTransport::Blocking(client, host, _auth) => {
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

    pub fn put<P: Serialize>(&self, path: String, payload: P) -> anyhow::Result<()> {
        match self {
            HttpTransport::Blocking(client, host, _auth) => {
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
            HttpTransport::Blocking(client, host, _auth) => {
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
            HttpTransport::Blocking(client, host, _auth) => {
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
