use super::super::{candidate::RepairCandidate, lists, scan, syntax_primitives};
use super::priorities::{
    PRI_ENUM_VARIANT_TRAILING_COMMA_DELETE, PRI_ENUM_VARIANT_TRAILING_COMMA_FIX, PRI_TYPE_FIELD_TRAILING_COMMA_DELETE,
    PRI_TYPE_FIELD_TRAILING_COMMA_FIX,
};
use super::scanner_context::item_body_keyword_before_brace;

pub(super) fn type_and_enum_field_list_repairs(source: &str, error_pos: usize) -> Vec<RepairCandidate> {
    let scan_pos = error_pos.min(source.len());
    let Some(open_pos) = syntax_primitives::find_unclosed_delimiter_before(source, scan_pos, b'{', b'}') else {
        return Vec::new();
    };
    let insert_at = scan::skip_ws(source, scan_pos);

    let mut out = Vec::new();
    match item_body_keyword_before_brace(source, open_pos) {
        Some("type") => {
            lists::trailing_separator_before_close_delimiter(
                source,
                scan_pos,
                insert_at,
                &mut out,
                b'{',
                b'}',
                |_source, open, _scan_pos| item_body_keyword_before_brace(source, open) == Some("type"),
                "field: word",
                PRI_TYPE_FIELD_TRAILING_COMMA_DELETE,
                PRI_TYPE_FIELD_TRAILING_COMMA_FIX,
                "removed trailing comma in type field list",
                "inserted placeholder type field after trailing comma",
            );
        }
        Some("enum") => {
            lists::trailing_separator_before_close_delimiter(
                source,
                scan_pos,
                insert_at,
                &mut out,
                b'{',
                b'}',
                |_source, open, _scan_pos| item_body_keyword_before_brace(source, open) == Some("enum"),
                "Value",
                PRI_ENUM_VARIANT_TRAILING_COMMA_DELETE,
                PRI_ENUM_VARIANT_TRAILING_COMMA_FIX,
                "removed trailing comma in enum variant list",
                "inserted placeholder enum variant after trailing comma",
            );
        }
        _ => {}
    }

    out
}
