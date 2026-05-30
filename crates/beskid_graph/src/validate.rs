use graphs_tui::DiagramWarning;

pub fn validate_mermaid(mermaid: &str) -> Vec<DiagramWarning> {
    graphs_tui::check("mermaid", mermaid).unwrap_or_default()
}
