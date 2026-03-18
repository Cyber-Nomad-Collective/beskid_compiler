use reqwest::Method;

use crate::client::PckgClient;
use crate::error::PckgError;
use crate::models::{ApiKeysListResponse, CreateApiKeyRequest, CreateApiKeyResponse};

impl PckgClient {
    pub async fn list_api_keys(&self) -> Result<Vec<ApiKeysListResponse>, PckgError> {
        self.send_no_body(Method::GET, "/api/keys", true).await
    }

    pub async fn create_api_key(
        &self,
        request: &CreateApiKeyRequest,
    ) -> Result<CreateApiKeyResponse, PckgError> {
        self.send_with_body(Method::POST, "/api/keys", request, true)
            .await
    }
}
