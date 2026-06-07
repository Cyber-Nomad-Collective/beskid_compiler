use anyhow::Result;
use beskid_codegen::services::lower_source;

#[test]
fn extern_invalid_abi_rejected() -> Result<()> {
    let src = r#"
[Extern(Abi:"Rust", Library:"libc.so.6")]
pub contract C { i64 getpid(); }

pub i64 Main() { return 0; }
"#;
    let err = lower_source(std::path::Path::new("<memory>"), src, false)
        .err()
        .expect("type checking should fail for invalid ABI");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("invalid extern ABI"),
        "unexpected message: {msg}"
    );
    Ok(())
}

#[test]
fn extern_missing_library_rejected() -> Result<()> {
    let src = r#"
[Extern(Abi:"C")]
pub contract C { i64 getpid(); }

pub i64 Main() { return 0; }
"#;
    let err = lower_source(std::path::Path::new("<memory>"), src, false)
        .err()
        .expect("type checking should fail for missing library");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("extern declaration missing library"),
        "unexpected message: {msg}"
    );
    Ok(())
}

#[test]
fn extern_disallowed_param_type_rejected() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C { i64 nope(string s); }

pub i64 Main() { return 0; }
"#;
    let err = lower_source(std::path::Path::new("<memory>"), src, false)
        .err()
        .expect("type checking should fail for disallowed param type");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("disallowed parameter type") && msg.contains("nope"),
        "unexpected message: {msg}"
    );
    Ok(())
}

#[test]
fn extern_ref_param_modifier_rejected_at_parse() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C { i64 nope(ref i64 p); }

pub i64 Main() { return 0; }
"#;
    let err = lower_source(std::path::Path::new("<memory>"), src, false)
        .err()
        .expect("parse should fail for removed ref parameter modifier");
    let msg = format!("{err:#}");
    assert!(msg.contains("parse error"), "unexpected message: {msg}");
    Ok(())
}
