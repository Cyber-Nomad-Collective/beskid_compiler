use serde::{Deserialize, Serialize};
use tower_lsp_server::Client;
use tower_lsp_server::ls_types::notification::Notification;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeskidStatusParams {
    pub source: String,
    pub phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    #[serde(default)]
    pub active: bool,
}

pub enum BeskidStatus {}

impl Notification for BeskidStatus {
    type Params = BeskidStatusParams;
    const METHOD: &'static str = "beskid/status";
}

pub async fn send_beskid_status(client: &Client, params: BeskidStatusParams) {
    client.send_notification::<BeskidStatus>(params).await;
}
