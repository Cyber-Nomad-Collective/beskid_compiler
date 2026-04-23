use crate::parsing::util::{assert_parse, assert_parse_fail};
use beskid_analysis::Rule;

#[test]
fn parses_item_rule() {
    assert_parse(Rule::InnerItem, "unit main() { return; }");
}

#[test]
fn rejects_item_rule() {
    assert_parse_fail(Rule::InnerItem, "main");
}

#[test]
fn parses_test_item_rule() {
    assert_parse(
        Rule::InnerItem,
        r#"
test Example {
    meta {
        tags = "fast";
        group = "parser";
    }
    skip {
        condition = false;
        reason = "enabled in CI";
    }
    return;
}
"#,
    );
}
