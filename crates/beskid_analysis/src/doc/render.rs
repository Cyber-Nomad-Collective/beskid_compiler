use super::ItemDocStructured;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDoc {
    pub markdown: String,
    pub structured: Option<ItemDocStructured>,
}
