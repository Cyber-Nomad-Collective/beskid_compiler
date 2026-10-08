use std::path::Path;

use super::path_inference::{module_path_from_generated_suffix, module_path_from_src_suffix};

#[test]
fn generated_file_suffix_keeps_the_complete_module_name() {
    assert_eq!(
        module_path_from_generated_suffix(
            Path::new("/packages/corelib/.generated/Core/Text/Regex/Generated.g.bd")),
        Some(vec!["Core".to_string(), "Text".to_string(), "Regex".to_string(), "Generated".to_string(),])
    );
}

#[test]
fn windows_generated_file_suffix_keeps_the_complete_module_name() {
    assert_eq!(
        module_path_from_generated_suffix(
            Path::new(r"C:\workspace\corelib\.generated\Core\Text\Regex\Generated.g.bd")),
        Some(vec!["Core".to_string(), "Text".to_string(), "Regex".to_string(), "Generated".to_string()])
    );
}

#[test]
fn windows_source_suffix_keeps_the_complete_module_name() {
    assert_eq!(
        module_path_from_src_suffix(Path::new(r"C:\workspace\corelib\src\Core\Text\Regex.bd")),
        Some(vec!["Core".to_string(), "Text".to_string(), "Regex".to_string()])
    );
}
