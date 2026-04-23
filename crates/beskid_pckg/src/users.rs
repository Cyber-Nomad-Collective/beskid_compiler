use reqwest::Method;

use crate::client::PckgClient;
use crate::error::PckgError;
use crate::models::{
    AuthActionResponse, BootstrapStatusResponse, CreateInitialAdminRequest, CurrentUserResponse,
    LoginUserRequest, RegisterUserRequest,
};

fn ensure_auth_action_success(
    response: AuthActionResponse,
) -> Result<AuthActionResponse, PckgError> {
    if response.success {
        Ok(response)
    } else {
        Err(PckgError::logical_failure(response.message.clone(), None))
    }
}

impl PckgClient {
    pub async fn get_bootstrap_status(&self) -> Result<BootstrapStatusResponse, PckgError> {
        self.send_no_body(Method::GET, "/api/users/bootstrap-status", false)
            .await
    }

    pub async fn create_initial_admin(
        &self,
        request: &CreateInitialAdminRequest,
    ) -> Result<AuthActionResponse, PckgError> {
        let response: AuthActionResponse = self
            .send_with_body(Method::POST, "/api/users/bootstrap-admin", request, false)
            .await?;
        ensure_auth_action_success(response)
    }

    pub async fn login_user(
        &self,
        request: &LoginUserRequest,
    ) -> Result<AuthActionResponse, PckgError> {
        let response: AuthActionResponse = self
            .send_with_body(Method::POST, "/api/users/login", request, false)
            .await?;
        ensure_auth_action_success(response)
    }

    pub async fn register_user(
        &self,
        request: &RegisterUserRequest,
    ) -> Result<AuthActionResponse, PckgError> {
        let response: AuthActionResponse = self
            .send_with_body(Method::POST, "/api/users/register", request, false)
            .await?;
        ensure_auth_action_success(response)
    }

    pub async fn current_user(&self) -> Result<CurrentUserResponse, PckgError> {
        self.send_no_body(Method::GET, "/api/users/me", false).await
    }

    pub async fn become_publisher(&self) -> Result<AuthActionResponse, PckgError> {
        let response: AuthActionResponse = self
            .send_no_body(Method::POST, "/api/users/become-publisher", true)
            .await?;
        ensure_auth_action_success(response)
    }
}
