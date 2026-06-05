use super::ItemDocStructured;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedDoc {
    pub markdown: String,
    pub structured: Option<ItemDocStructured>,
}
