use std::path::Path;

use super::path_inference::module_path_from_generated_suffix;

#[test]
fn generated_file_suffix_keeps_the_complete_module_name() {
    assert_eq!(
        module_path_from_generated_suffix(
            Path::new("/packages/corelib/.generated/Core/Text/Regex/Generated.g.bd"),
            false,
        ),
        Some(vec!["Core".to_string(), "Text".to_string(), "Regex".to_string(), "Generated".to_string(),])
    );
}
