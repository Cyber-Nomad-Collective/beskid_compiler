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

#[test]
fn source_suffix_ignores_src_components_above_the_package_root() {
    assert_eq!(
        module_path_from_src_suffix(Path::new(
            "/woodpecker/src/github.com/Org/beskid/compiler/corelib/packages/foundation/src/Core/Results/Result.bd"
        )),
        Some(vec!["Core".to_string(), "Results".to_string(), "Result".to_string()])
    );
    assert_eq!(
        module_path_from_src_suffix(Path::new("/home/user/src/app/src/Main.bd")),
        Some(vec!["Main".to_string()])
    );
}

#[test]
fn windows_source_suffix_ignores_src_components_above_the_package_root() {
    assert_eq!(
        module_path_from_src_suffix(Path::new(r"C:\src\workspace\corelib\src\Core\Text\Regex.bd")),
        Some(vec!["Core".to_string(), "Text".to_string(), "Regex".to_string()])
    );
}
