use reqwest::Method;

use crate::client::PckgClient;
use crate::error::PckgError;
use crate::models::{
    ApiKeysListResponse, CreateApiKeyRequest, CreateApiKeyResponse, RevokeApiKeyResponse,
};

fn ensure_create_key_success(
    response: CreateApiKeyResponse,
) -> Result<CreateApiKeyResponse, PckgError> {
    if response.success {
        Ok(response)
    } else {
        Err(PckgError::logical_failure(response.message.clone(), None))
    }
}

fn ensure_revoke_success(
    response: RevokeApiKeyResponse,
) -> Result<RevokeApiKeyResponse, PckgError> {
    if response.success {
        Ok(response)
    } else {
        Err(PckgError::logical_failure(response.message.clone(), None))
    }
}

impl PckgClient {
    pub async fn list_api_keys(&self) -> Result<Vec<ApiKeysListResponse>, PckgError> {
        self.send_no_body(Method::GET, "/api/keys", true).await
    }

    pub async fn create_api_key(
        &self,
        request: &CreateApiKeyRequest,
    ) -> Result<CreateApiKeyResponse, PckgError> {
        let response: CreateApiKeyResponse = self
            .send_with_body(Method::POST, "/api/keys", request, true)
            .await?;
        ensure_create_key_success(response)
    }

    pub async fn revoke_api_key(&self, key_id: &str) -> Result<RevokeApiKeyResponse, PckgError> {
        let path = format!("/api/keys/{key_id}/revoke");
        let response: RevokeApiKeyResponse = self.send_no_body(Method::POST, &path, true).await?;
        ensure_revoke_success(response)
    }
}
