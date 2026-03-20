use anyhow::Result;
use beskid_codegen::services::lower_source;

#[test]
fn extern_invalid_abi_rejected() -> Result<()> {
    let src = r#"
[Extern(Abi:"Rust", Library:"libc.so.6")]
pub contract C { i64 getpid(); }

pub i64 main() { return 0; }
"#;
    let err = lower_source(std::path::Path::new("<memory>"), src, false)
        .err()
        .expect("type checking should fail for invalid ABI");
    let msg = format!("{err:?}");
    assert!(msg.contains("ExternInvalidAbi"));
    Ok(())
}

#[test]
fn extern_missing_library_rejected() -> Result<()> {
    let src = r#"
[Extern(Abi:"C")]
pub contract C { i64 getpid(); }

pub i64 main() { return 0; }
"#;
    let err = lower_source(std::path::Path::new("<memory>"), src, false)
        .err()
        .expect("type checking should fail for missing library");
    let msg = format!("{err:?}");
    assert!(msg.contains("ExternMissingLibrary"));
    Ok(())
}

#[test]
fn extern_disallowed_param_type_rejected() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C { i64 nope(string s); }

pub i64 main() { return 0; }
"#;
    let err = lower_source(std::path::Path::new("<memory>"), src, false)
        .err()
        .expect("type checking should fail for disallowed param type");
    let msg = format!("{err:?}");
    assert!(msg.contains("ExternDisallowedParamType"));
    Ok(())
}

#[test]
fn extern_disallowed_ref_param_type_rejected() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C { i64 nope(ref i64 p); }

pub i64 main() { return 0; }
"#;
    let err = lower_source(std::path::Path::new("<memory>"), src, false)
        .err()
        .expect("type checking should fail for disallowed ref param type");
    let msg = format!("{err:?}");
    assert!(msg.contains("ExternDisallowedParamType"));
    Ok(())
}

