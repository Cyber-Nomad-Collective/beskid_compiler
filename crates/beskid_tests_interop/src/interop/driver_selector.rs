//! The installed compiler driver is never admitted from a Cargo build tree.
//!
//! The admission side (a private copy of a qualified installation prefix) needs the installation
//! and runs in the release harness (`beskid_tests_release`, `glue_owned`).

#[test]
fn v06_glue_driver_denied_from_build_tree() {
    let error = match beskid_abi::compiler_driver::installed_compiler_driver() {
        Ok(driver) => panic!("driver admitted from build tree: {}", driver.path().display()),
        Err(error) => error,
    };
    assert!(!error.is_empty());
}
