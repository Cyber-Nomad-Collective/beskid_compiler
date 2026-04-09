use std::time::Duration;

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PckgAuth {
    BearerToken(String),
    PublisherApiKey(String),
}

#[derive(Debug, Clone)]
pub struct PckgClientConfig {
    pub base_url: Url,
    pub auth: Option<PckgAuth>,
    pub api_key_header_name: String,
    pub timeout: Duration,
    pub user_agent: String,
}

impl PckgClientConfig {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self, url::ParseError> {
        let mut url = Url::parse(base_url.as_ref())?;
        if !url.path().ends_with('/') {
            let path = format!("{}/", url.path().trim_end_matches('/'));
            url.set_path(&path);
        }

        Ok(Self {
            base_url: url,
            auth: None,
            api_key_header_name: "X-API-Key".to_string(),
            timeout: Duration::from_secs(30),
            user_agent: format!("beskid-pckg-client/{}", env!("CARGO_PKG_VERSION")),
        })
    }

    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(PckgAuth::BearerToken(token.into()));
        self
    }

    pub fn with_publisher_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.auth = Some(PckgAuth::PublisherApiKey(api_key.into()));
        self
    }

    pub fn with_api_key_header_name(mut self, header_name: impl Into<String>) -> Self {
        self.api_key_header_name = header_name.into();
        self
    }

    pub fn with_auth_token(self, token: impl Into<String>) -> Self {
        self.with_bearer_token(token)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }
}
