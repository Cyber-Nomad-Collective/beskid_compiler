//! Source-scoped Corelib imports for the versioned browser target.
use super::corelib_services::{CorelibService, CorelibServiceAbiType};

pub const HTTP_SOURCE: &str = "Web/WebClient.bd";
pub const ENTROPY_SOURCE: &str = "Crypto/EntropyPlatform.bd";
pub const WS_SOURCE: &str = "WebSocket/WsConnection.bd";

pub struct Source {
    pub logical_path: &'static str,
    pub package: &'static str,
    pub native: &'static str,
    pub browser: &'static str,
}

pub const SOURCES: &[Source] = &[
    Source {
        logical_path: ENTROPY_SOURCE,
        package: "crypto",
        native: include_str!("../../../../corelib/packages/crypto/src/Crypto/EntropyPlatform.bd"),
        browser: include_str!("../../../../corelib/packages/crypto/browser/EntropyPlatform.bd"),
    },
    Source {
        logical_path: HTTP_SOURCE,
        package: "web",
        native: include_str!("../../../../corelib/packages/web/src/Web/WebClient.bd"),
        browser: include_str!("../../../../corelib/packages/web/browser/WebClient.bd"),
    },
    Source {
        logical_path: WS_SOURCE,
        package: "websocket",
        native: include_str!("../../../../corelib/packages/websocket/src/WebSocket/WsConnection.bd"),
        browser: include_str!("../../../../corelib/packages/websocket/browser/WsConnection.bd"),
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Integer,
    String,
    Bytes,
    Unit,
}

impl Value {
    pub fn corelib_abi(self) -> CorelibServiceAbiType {
        match self {
            Self::Integer => CorelibServiceAbiType::I64,
            Self::String => CorelibServiceAbiType::String,
            Self::Bytes => CorelibServiceAbiType::Pointer,
            Self::Unit => CorelibServiceAbiType::Void,
        }
    }
}

pub struct Operation {
    pub service: CorelibService,
    pub capability: &'static str,
    pub parameters: &'static [Value],
    pub result: Value,
    pub suspends: bool,
}

macro_rules! operation {
    ($capability:literal, $source:expr, $name:literal, $symbol:literal, [$($arg:ident),*], $result:ident, $suspends:literal) => {
        Operation {
            service: CorelibService { name: $name, symbol: $symbol, source_path: $source },
            capability: $capability,
            parameters: &[$(Value::$arg),*],
            result: Value::$result,
            suspends: $suspends,
        }
    };
}

const OPERATIONS: &[Operation] = &[
    operation!("entropy", ENTROPY_SOURCE, "__browser_entropy_fill", "beskid_rt_browser_entropy_fill", [Bytes], Integer, false),
    operation!(
        "http",
        HTTP_SOURCE,
        "__browser_http_fetch",
        "beskid_rt_browser_http_fetch",
        [String, String, String, Bytes, Integer, Integer, Integer, Integer],
        Integer,
        true
    ),
    operation!(
        "http",
        HTTP_SOURCE,
        "__browser_http_status",
        "beskid_rt_browser_http_status",
        [Integer],
        Integer,
        false
    ),
    operation!("http", HTTP_SOURCE, "__browser_http_reason", "beskid_rt_browser_http_reason", [Integer], String, false),
    operation!(
        "http",
        HTTP_SOURCE,
        "__browser_http_header_count",
        "beskid_rt_browser_http_header_count",
        [Integer],
        Integer,
        false
    ),
    operation!(
        "http",
        HTTP_SOURCE,
        "__browser_http_header_name",
        "beskid_rt_browser_http_header_name",
        [Integer, Integer],
        String,
        false
    ),
    operation!(
        "http",
        HTTP_SOURCE,
        "__browser_http_header_value",
        "beskid_rt_browser_http_header_value",
        [Integer, Integer],
        String,
        false
    ),
    operation!(
        "http",
        HTTP_SOURCE,
        "__browser_http_body_len",
        "beskid_rt_browser_http_body_len",
        [Integer],
        Integer,
        false
    ),
    operation!(
        "http",
        HTTP_SOURCE,
        "__browser_http_body_byte",
        "beskid_rt_browser_http_body_byte",
        [Integer, Integer],
        Integer,
        false
    ),
    operation!("http", HTTP_SOURCE, "__browser_http_drop", "beskid_rt_browser_http_drop", [Integer], Unit, false),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_open",
        "beskid_rt_browser_ws_open",
        [String, String, Integer, Integer, Integer, Integer],
        Integer,
        true
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_send_text",
        "beskid_rt_browser_ws_send_text",
        [Integer, String],
        Integer,
        false
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_send_binary",
        "beskid_rt_browser_ws_send_binary",
        [Integer, Bytes],
        Integer,
        false
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_receive",
        "beskid_rt_browser_ws_receive",
        [Integer],
        Integer,
        true
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_message_kind",
        "beskid_rt_browser_ws_message_kind",
        [Integer],
        Integer,
        false
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_message_code",
        "beskid_rt_browser_ws_message_code",
        [Integer],
        Integer,
        false
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_message_text",
        "beskid_rt_browser_ws_message_text",
        [Integer],
        String,
        false
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_message_len",
        "beskid_rt_browser_ws_message_len",
        [Integer],
        Integer,
        false
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_message_byte",
        "beskid_rt_browser_ws_message_byte",
        [Integer, Integer],
        Integer,
        false
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_message_drop",
        "beskid_rt_browser_ws_message_drop",
        [Integer],
        Unit,
        false
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_close",
        "beskid_rt_browser_ws_close",
        [Integer, Integer, String],
        Integer,
        true
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_protocol",
        "beskid_rt_browser_ws_protocol",
        [Integer],
        String,
        false
    ),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_resource",
        "beskid_rt_browser_ws_resource",
        [Integer],
        String,
        false
    ),
    operation!("websocket", WS_SOURCE, "__browser_ws_origin", "beskid_rt_browser_ws_origin", [Integer], String, false),
    operation!("websocket", WS_SOURCE, "__browser_ws_state", "beskid_rt_browser_ws_state", [Integer], Integer, false),
    operation!(
        "websocket",
        WS_SOURCE,
        "__browser_ws_close_code",
        "beskid_rt_browser_ws_close_code",
        [Integer],
        Integer,
        false
    ),
];

pub fn operations() -> &'static [Operation] {
    OPERATIONS
}
pub fn operation(symbol: &str) -> Option<&'static Operation> {
    operations().iter().find(|operation| operation.service.symbol == symbol)
}
