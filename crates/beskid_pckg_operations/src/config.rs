#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CaptchaConfig {
    pub site_key: Option<String>,
    pub project_id: Option<String>,
    pub api_key: Option<String>,
    pub minimum_score_milli: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct AuthHubConfig {
    pub hub_public_url: Option<String>,
    pub public_url: Option<String>,
    pub pairing_approver_login: Option<String>,
    pub github_sync_token: Option<String>,
    pub service_token: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct OperationsConfig {
    pub captcha: CaptchaConfig,
    pub auth_hub: AuthHubConfig,
    pub session_secret: Option<String>,
    pub require_structured_api_doc: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigValidationError {
    IncompleteCaptcha,
    InvalidCaptchaMinimumScore,
    IncompleteAuthHubSession,
    InvalidHubPublicUrl,
    InvalidPublicUrl,
}

impl OperationsConfig {
    pub fn for_test() -> Self {
        Self { require_structured_api_doc: true, ..Self::default() }
    }

    /// Validates the operational configuration before adapters bind network or secret clients.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        let captcha_values = [&self.captcha.site_key, &self.captcha.project_id, &self.captcha.api_key];
        let configured_captcha_values = captcha_values.iter().filter(|value| is_present(value)).count();
        if configured_captcha_values != 0 && configured_captcha_values != captcha_values.len() {
            return Err(ConfigValidationError::IncompleteCaptcha);
        }
        if self.captcha.minimum_score_milli > 1000 {
            return Err(ConfigValidationError::InvalidCaptchaMinimumScore);
        }
        if is_present(&self.auth_hub.service_token) != is_present(&self.session_secret) {
            return Err(ConfigValidationError::IncompleteAuthHubSession);
        }
        if is_present(&self.auth_hub.hub_public_url) && !is_http_url(self.auth_hub.hub_public_url.as_deref().unwrap()) {
            return Err(ConfigValidationError::InvalidHubPublicUrl);
        }
        if is_present(&self.auth_hub.public_url) && !is_http_url(self.auth_hub.public_url.as_deref().unwrap()) {
            return Err(ConfigValidationError::InvalidPublicUrl);
        }
        Ok(())
    }
}

fn is_present(value: &Option<String>) -> bool {
    value.as_deref().is_some_and(|value| !value.trim().is_empty())
}

fn is_http_url(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("https://") || value.starts_with("http://")
}
