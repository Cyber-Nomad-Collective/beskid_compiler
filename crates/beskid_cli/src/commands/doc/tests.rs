use super::{DocEntry, render_structure_tree};

#[test]
fn structure_tree_renders_nested_paths() {
    let entries = vec![
        DocEntry { qualified_name: "util::math::sum".to_string(), kind: "function".to_string(), doc_markdown: None },
        DocEntry { qualified_name: "util::math::Vec2".to_string(), kind: "type".to_string(), doc_markdown: None },
    ];

    let tree = render_structure_tree(&entries);
    assert!(tree.contains("- `util`"));
    assert!(tree.contains("- `math`"));
    assert!(tree.contains("`util::math::sum` (`function`)"));
    assert!(tree.contains("`util::math::Vec2` (`type`)"));
}
