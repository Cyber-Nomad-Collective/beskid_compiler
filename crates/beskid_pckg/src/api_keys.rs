use reqwest::Method;

use crate::client::PckgClient;
use crate::error::PckgError;
use crate::models::{
    ApiKeysListResponse, CreateApiKeyRequest, CreateApiKeyResponse, RevokeApiKeyResponse,
};

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

    pub async fn revoke_api_key(&self, key_id: &str) -> Result<RevokeApiKeyResponse, PckgError> {
        let path = format!("/api/keys/{key_id}/revoke");
        self.send_no_body(Method::POST, &path, true).await
    }
}
