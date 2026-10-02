use reqwest::blocking::{Client, Response};
use serde::{Serialize, de::DeserializeOwned};

type Host = String;

#[derive(Debug)]
pub enum Auth {
    Token(String),
    None,
}

#[derive(Debug)]
pub struct HttpClient {
    client: Client,
    host: Host,
    // stored but not yet applied to outgoing requests
    #[allow(dead_code)]
    auth: Auth,
}

impl HttpClient {
    pub fn new(host: String, auth: Auth) -> HttpClient {
        HttpClient {
            client: Client::new(),
            host,
            auth,
        }
    }

    pub fn get<T: DeserializeOwned>(&self, path: String) -> anyhow::Result<T> {
        match self.client.get(format!("{}{path}", self.host)).send() {
            Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }

    /// Same as [`Self::get`], but with the `X-Flagrant-Identity` header set - needed for the
    /// public evaluation endpoint, which identifies the caller via that header rather than a
    /// path segment.
    pub fn get_with_identity<T: DeserializeOwned>(
        &self,
        path: String,
        identity: &str,
    ) -> anyhow::Result<T> {
        match self
            .client
            .get(format!("{}{path}", self.host))
            .header("X-Flagrant-Identity", identity)
            .send()
        {
            Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
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
            .post(format!("{}{path}", self.host))
            .json(&payload)
            .send();

        match result {
            Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }

    pub fn put<P: Serialize>(&self, path: String, payload: P) -> anyhow::Result<()> {
        let result = self
            .client
            .put(format!("{}{path}", self.host))
            .json(&payload)
            .send();

        match result {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }

    pub fn patch<P: Serialize, T: DeserializeOwned>(
        &self,
        path: String,
        payload: P,
    ) -> anyhow::Result<T> {
        let result = self
            .client
            .patch(format!("{}{path}", self.host))
            .json(&payload)
            .send();

        match result {
            Ok(response) if response.status().is_success() => Ok(response.json::<T>()?),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }

    pub fn delete(&self, path: String) -> anyhow::Result<Response> {
        let result = self.client.delete(format!("{}{path}", self.host)).send();

        match result {
            Ok(response) if response.status().is_success() => Ok(response),
            Ok(response) => Err(anyhow::anyhow!(response.text()?)),
            Err(err) => Err(err.into()),
        }
    }
}
