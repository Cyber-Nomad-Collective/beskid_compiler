use tower_lsp_server::Client;
use tower_lsp_server::ls_types::MessageType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientLogFilter {
    Error,
    Warning,
    Info,
    Log,
    Debug,
}

impl ClientLogFilter {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "error" => Self::Error,
            "warn" | "warning" => Self::Warning,
            "info" => Self::Info,
            "log" => Self::Log,
            "debug" | "trace" | "verbose" => Self::Debug,
            _ => Self::Info,
        }
    }

    /// Minimum importance to forward (higher = fewer messages).
    fn min_importance(self) -> u8 {
        match self {
            Self::Error => 4,
            Self::Warning => 3,
            Self::Info => 2,
            Self::Log | Self::Debug => 1,
        }
    }

    fn importance(msg: MessageType) -> u8 {
        match msg {
            MessageType::ERROR => 4,
            MessageType::WARNING => 3,
            MessageType::INFO => 2,
            MessageType::LOG => 1,
            _ => 1,
        }
    }

    pub fn allows(self, msg: MessageType) -> bool {
        Self::importance(msg) >= self.min_importance()
    }
}

pub async fn client_log(client: &Client, filter: ClientLogFilter, msg: MessageType, message: String) {
    if filter.allows(msg) {
        client.log_message(msg, message).await;
    }
}
