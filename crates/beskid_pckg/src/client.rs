use reqwest::Method;
use reqwest::multipart;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::{PckgAuth, PckgClientConfig};
use crate::error::PckgError;

#[derive(Debug, Clone)]
pub struct PckgClient {
    config: PckgClientConfig,
    http: reqwest::Client,
}

impl PckgClient {
    pub fn new(config: PckgClientConfig) -> Result<Self, PckgError> {
        let http = reqwest::Client::builder().timeout(config.timeout).user_agent(config.user_agent.clone()).build()?;

        Ok(Self { config, http })
    }

    pub fn config(&self) -> &PckgClientConfig {
        &self.config
    }

    pub(crate) async fn send_no_body<R>(&self, method: Method, path: &str, require_auth: bool) -> Result<R, PckgError>
    where
        R: DeserializeOwned,
    {
        let request = self.build_request(method, path, require_auth)?;
        self.execute_json(request).await
    }

    pub(crate) async fn send_with_body<B, R>(
        &self,
        method: Method,
        path: &str,
        body: &B,
        require_auth: bool,
    ) -> Result<R, PckgError>
    where
        B: Serialize,
        R: DeserializeOwned,
    {
        let request = self.build_request(method, path, require_auth)?.json(body);
        self.execute_json(request).await
    }

    pub(crate) async fn send_multipart<R>(
        &self,
        method: Method,
        path: &str,
        form: multipart::Form,
        require_auth: bool,
    ) -> Result<R, PckgError>
    where
        R: DeserializeOwned,
    {
        let request = self.build_request(method, path, require_auth)?.multipart(form);
        self.execute_json(request).await
    }

    /// Send a request and return the raw [`reqwest::Response`] without
    /// consuming the body stream. The caller is responsible for streaming
    /// bytes from the response (e.g. via `.bytes_stream()`) and MUST
    /// consume or drop the response to release the connection.
    pub(crate) async fn send_streaming(
        &self,
        method: Method,
        path: &str,
        require_auth: bool,
    ) -> Result<reqwest::Response, PckgError> {
        let response = self.build_request(method, path, require_auth)?.send().await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(PckgError::from_api_error(status, body));
        }
        Ok(response)
    }

    fn build_request(
        &self,
        method: Method,
        path: &str,
        require_auth: bool,
    ) -> Result<reqwest::RequestBuilder, PckgError> {
        let url = self.config.base_url.join(path.trim_start_matches('/'))?;

        let request = self.http.request(method, url);

        match (require_auth, self.config.auth.as_ref()) {
            (_, Some(PckgAuth::BearerToken(token))) => Ok(request.bearer_auth(token)),
            (_, Some(PckgAuth::PublisherApiKey(api_key))) => {
                Ok(request.header(self.config.api_key_header_name.as_str(), api_key))
            }
            (true, None) => Err(PckgError::MissingAuthToken),
            (false, None) => Ok(request),
        }
    }

    async fn execute_json<R>(&self, request: reqwest::RequestBuilder) -> Result<R, PckgError>
    where
        R: DeserializeOwned,
    {
        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(PckgError::from_api_error(status, body));
        }

        serde_json::from_str(&body).map_err(|source| PckgError::Api {
            status,
            message: format!("invalid JSON response: {source}"),
            body: Some(body),
        })
    }

}
