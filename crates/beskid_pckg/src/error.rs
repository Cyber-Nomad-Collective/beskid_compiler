use reqwest::StatusCode;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PckgError {
    #[error("failed to build request URL: {0}")]
    Url(#[from] url::ParseError),

    #[error("HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("authentication token is required for this endpoint")]
    MissingAuthToken,

    #[error("failed to initialize async runtime: {0}")]
    RuntimeInit(String),

    #[error("API request failed with status {status}: {message}")]
    Api {
        status: StatusCode,
        message: String,
        body: Option<String>,
    },
}

impl PckgError {
    pub(crate) fn from_api_error(status: StatusCode, body: String) -> Self {
        let message = extract_api_message(&body)
            .unwrap_or_else(|| status.canonical_reason().unwrap_or("request failed").to_string());

        Self::Api {
            status,
            message,
            body: Some(body),
        }
    }
}

fn extract_api_message(body: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(body).ok()?;
    value
        .get("message")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

