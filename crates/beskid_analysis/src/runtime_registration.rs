//! Runtime handler registration metadata merged from manifest `language_handler` rows.

include!("generated/runtime_handlers.inc.rs");

/// Look up a language handler spec by its Beskid handler path segments.
pub fn runtime_handler_for_path(path: &[String]) -> Option<&'static RuntimeHandlerSpec> {
    RUNTIME_HANDLER_SPECS
        .iter()
        .find(|spec| path_matches(spec.handler_path, path))
}

/// Validate that a `[Runtime(DispatchTag: …)]` tag matches the manifest row for `dispatch_key`.
pub fn validate_runtime_handler_tag(dispatch_key: &str, tag: u32) -> bool {
    RUNTIME_HANDLER_SPECS
        .iter()
        .any(|spec| spec.dispatch_key == dispatch_key && spec.tag == tag)
}

fn path_matches(expected: &[&str], actual: &[String]) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    expected
        .iter()
        .zip(actual.iter())
        .all(|(left, right)| *left == right)
}

/// Manifest return group label for `[Runtime(Returns: …)]` legality checks.
pub fn runtime_return_group_label(return_group: &str) -> &'static str {
    match return_group {
        "i64" => "I64",
        "usize" => "USize",
        "ptr" => "Ptr",
        "unit" => "Unit",
        _ => "Never",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_handler_specs_include_cohort_one() {
        assert!(validate_runtime_handler_tag("bytes_compare", 0));
        assert!(validate_runtime_handler_tag("bytes_get", 1));
        assert!(validate_runtime_handler_tag("str_eq", 42));
        let path = vec![
            "Runtime".to_string(),
            "Handlers".to_string(),
            "Bytes".to_string(),
            "Compare".to_string(),
        ];
        let spec = runtime_handler_for_path(&path).expect("bytes handler path");
        assert_eq!(spec.dispatch_key, "bytes_compare");
        let get_path = vec![
            "Runtime".to_string(),
            "Handlers".to_string(),
            "Bytes".to_string(),
            "Get".to_string(),
        ];
        let get_spec = runtime_handler_for_path(&get_path).expect("bytes_get handler path");
        assert_eq!(get_spec.dispatch_key, "bytes_get");
    }
}
