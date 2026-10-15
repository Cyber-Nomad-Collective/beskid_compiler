//! Exact executable receipts, independent of linker command formatting or global CC state.
#[cfg(unix)]
mod unix {
    use beskid_aot::linker::{LinkToolInvocation, run_link_tool};
    use sha2::{Digest, Sha256};
    use std::{fs, os::unix::fs::PermissionsExt};

    fn tool(root: &std::path::Path, body: &str) -> std::path::PathBuf {
        let path = root.join("link-tool");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn v06_link_tool_receipt_names_exact_executable_and_bytes() {
        let root = tempfile::tempdir().unwrap();
        let path = tool(root.path(), "printf linked");
        let invocation = LinkToolInvocation::new(path.as_os_str());
        let (output, receipt) = run_link_tool(&invocation, root.path(), None).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"linked");
        assert_eq!(receipt.executable, fs::canonicalize(&path).unwrap());
        assert_eq!(receipt.sha256, format!("{:x}", Sha256::digest(fs::read(&path).unwrap())));
    }

    #[test]
    fn v06_link_tool_changed_during_invocation_cannot_issue_receipt() {
        let root = tempfile::tempdir().unwrap();
        let path = tool(root.path(), "printf '# changed\\n' >> \"$0\"");
        let error = run_link_tool(&LinkToolInvocation::new(path.as_os_str()), root.path(), None).unwrap_err();
        assert!(error.to_string().contains("changed"), "{error}");
    }
    #[test]
    fn v06_link_tool_explicit_path_cwd_and_overrides_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        let path = tool(root.path(), r#"printf '%s:%s' "$LINK_FIXTURE" "$PWD""#);
        let mut invocation = LinkToolInvocation::new("link-tool");
        invocation.current_dir = Some(root.path().to_path_buf());
        invocation.environment = vec![
            ("PATH".into(), Some(root.path().as_os_str().to_os_string())),
            ("LINK_FIXTURE".into(), Some("exact".into())),
        ];
        let (output, receipt) = run_link_tool(&invocation, root.path(), None).unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("exact:{}", fs::canonicalize(root.path()).unwrap().display())
        );
        assert_eq!(receipt.executable, fs::canonicalize(path).unwrap());
    }
}
