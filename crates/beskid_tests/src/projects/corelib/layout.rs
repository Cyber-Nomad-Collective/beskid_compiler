use std::fs;

use super::{
    corelib_root, corelib_workspace_root, expected_corelib_workspace_sources, foundation_src,
};

#[test]
fn checked_in_corelib_template_has_manifest_and_prelude() {
    let template_root = corelib_root();
    let manifest = template_root.join("Project.proj");
    let prelude = template_root.join("src/Prelude.bd");

    assert!(
        manifest.is_file(),
        "missing corelib manifest: {}",
        manifest.display()
    );
    assert!(
        prelude.is_file(),
        "missing corelib prelude: {}",
        prelude.display()
    );
}

#[test]
fn checked_in_corelib_template_is_resolved_from_corelib_submodule() {
    let root = corelib_root();
    assert!(
        root.ends_with("beskid_corelib"),
        "expected beskid_corelib directory, got {}",
        root.display()
    );
}

#[test]
fn checked_in_corelib_workspace_declares_workspace_manifest() {
    let ws = corelib_workspace_root().join("Workspace.proj");
    assert!(
        ws.is_file(),
        "missing corelib workspace manifest: {}",
        ws.display()
    );
    let raw = std::fs::read_to_string(&ws).expect("read Workspace.proj");
    assert!(
        raw.contains("workspace {"),
        "Workspace.proj should open a workspace block"
    );
}

#[test]
fn checked_in_corelib_template_declares_corelib_project_name() {
    let root = corelib_root();
    let manifest = std::fs::read_to_string(root.join("Project.proj")).expect("read manifest");
    assert!(
        manifest.contains("name = \"corelib\""),
        "expected corelib package identity in Project.proj"
    );
}

#[test]
fn checked_in_corelib_template_has_mvp_module_files() {
    let root = corelib_workspace_root();

    for relative in expected_corelib_workspace_sources() {
        let path = root.join(relative);
        assert!(
            path.is_file(),
            "missing corelib source file: {}",
            path.display()
        );
    }
}

#[test]
fn compiler_sdk_syntax_node_files_track_inventory() {
    let inv_path = super::compiler_sdk_src().join("Beskid/Syntax/Nodes/_inventory.txt");
    assert!(
        inv_path.is_file(),
        "missing syntax node inventory: {}",
        inv_path.display()
    );
    let raw = fs::read_to_string(&inv_path).expect("read _inventory.txt");
    let nodes_dir = inv_path.parent().expect("Nodes dir");
    for line in raw.lines() {
        let name = line.trim();
        if name.is_empty() {
            continue;
        }
        let node_file = nodes_dir.join(format!("{name}.bd"));
        assert!(
            node_file.is_file(),
            "inventory lists {name} but missing {}",
            node_file.display()
        );
    }
}

#[test]
fn compiler_sdk_syntax_inventory_excludes_removed_meta_definition() {
    let inv_path = super::compiler_sdk_src().join("Beskid/Syntax/Nodes/_inventory.txt");
    let raw = fs::read_to_string(&inv_path).expect("read _inventory.txt");

    assert!(
        !raw.lines().any(|line| line.trim() == "MetaDefinition"),
        "MetaDefinition was removed from the syntax SDK inventory"
    );
    assert!(
        !inv_path
            .parent()
            .expect("Nodes dir")
            .join("MetaDefinition.bd")
            .exists(),
        "MetaDefinition syntax node mirror should not be checked in"
    );
}

#[test]
fn checked_in_corelib_foundation_package_has_manifest() {
    let p = foundation_src().parent().expect("src").join("Project.proj");
    assert!(p.is_file(), "missing foundation manifest: {}", p.display());
}

#[test]
fn checked_in_corelib_template_has_beskid_tests_project() {
    let root = corelib_root();
    let tests_project = root.join("tests/corelib_tests/Project.proj");
    let write_tests = root.join("tests/corelib_tests/src/system/SyscallWriteTests.bd");
    let api_tests = root.join("tests/corelib_tests/src/system/SyscallApiTests.bd");
    let ergonomics_tests = root.join("tests/corelib_tests/src/system/SyscallErgonomicsTests.bd");
    assert!(
        tests_project.is_file(),
        "missing corelib tests project manifest: {}",
        tests_project.display()
    );
    assert!(
        write_tests.is_file(),
        "missing syscall write tests: {}",
        write_tests.display()
    );
    assert!(
        api_tests.is_file(),
        "missing syscall api tests: {}",
        api_tests.display()
    );
    assert!(
        ergonomics_tests.is_file(),
        "missing syscall ergonomics tests: {}",
        ergonomics_tests.display()
    );
}

#[test]
fn checked_in_corelib_tests_project_uses_unique_name_and_declares_targets() {
    let manifest = std::fs::read_to_string(corelib_root().join("tests/corelib_tests/Project.proj"))
        .expect("read corelib_tests Project.proj");
    assert!(
        manifest.contains("name = \"corelib_tests\""),
        "corelib test harness must use project name corelib_tests (not corelib) to avoid recursive obj/ paths"
    );
    assert!(
        !manifest.contains("name = \"corelib\""),
        "corelib_tests Project.proj must not reuse aggregate package name corelib"
    );
    assert!(
        manifest.contains("dependency \"corelib\""),
        "corelib_tests should path-depend on aggregate beskid_corelib (package corelib)"
    );
    assert!(
        manifest.contains("path = \"../..\""),
        "corelib_tests path dependency should reference beskid_corelib root"
    );
    for target in [
        "SystemSyscallWriteTests",
        "SystemSyscallApiTests",
        "SystemSyscallErgonomicsTests",
        "SystemOutputWriteLineTests",
        "SystemInputReadTests",
        "SystemErrorWriteTests",
        "SystemFsTests",
        "SystemPathTests",
        "ConsoleAnsiEscapeTests",
        "ConsoleAnsiStyleChainTests",
        "ConsoleAnsiSgrGoldenTests",
        "ConsoleAnsiBuildersTests",
        "ConsoleFormatMarkdownTests",
        "ConsoleFormatAttributesTests",
        "ConsoleFormatScanTests",
        "ConsoleCapabilitiesTests",
        "ConsoleTerminalPlatformTests",
        "ConsoleFacadeTests",
        "ConsoleMessageChannelTests",
        "ConsoleStyleTests",
        "ConsoleControlsPanelTests",
        "ConsoleControlsProgressBarTests",
        "ConsoleControlsLayoutTests",
        "ConsoleControlsFrameTests",
        "ConsoleRenderContextTests",
        "CoreResultsTests",
        "CollectionsArrayTests",
        "CollectionsTier1Tests",
        "CollectionsListTests",
        "CollectionsMapTests",
        "CollectionsSetTests",
        "CollectionsQueueTests",
        "CollectionsStackTests",
        "ConcurrencyChannelApiTests",
        "ConcurrencyMutexTryLockTests",
        "ConcurrencyClockTests",
        "ConcurrencyHubRegisterTests",
        "ConcurrencyFiberHandleTests",
    ] {
        assert!(
            manifest.contains(&format!("target \"{target}\"")),
            "corelib_tests manifest missing target \"{target}\""
        );
    }
}

#[test]
fn corelib_collections_sources_carry_api_shape_tier_directives() {
    let collections_root = super::foundation_src().join("Collections");
    let mut missing: Vec<String> = Vec::new();
    for file in [
        "Array.bd",
        "List.bd",
        "Map.bd",
        "Set.bd",
        "Queue.bd",
        "Stack.bd",
    ] {
        let path = collections_root.join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read corelib collection source {}", path.display()));
        if !text.contains("@tier(") {
            missing.push(file.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "corelib collection sources without @tier(...) directive: {missing:?}"
    );
}

#[test]
fn corelib_system_streams_carry_api_shape_tier_directives() {
    let runtime_root = super::runtime_src().join("System");
    let mut missing: Vec<String> = Vec::new();
    for file in [
        "Input.bd",
        "Output.bd",
        "Error.bd",
        "Syscall.bd",
        "FS.bd",
        "Path.bd",
    ] {
        let path = runtime_root.join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read corelib system source {}", path.display()));
        if !text.contains("@tier(") {
            missing.push(file.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "corelib System.* sources without @tier(...) directive: {missing:?}"
    );
}

#[test]
fn checked_in_corelib_tier_metadata_round_trips_through_api_json() {
    use beskid_analysis::doc::{
        ApiDocItem, ApiLocation, TIER_STANDARD, TIER_SUPPORTED, TIER_UNSTABLE,
        resolve_item_tiers,
    };

    fn item(id: usize, parent: Option<usize>, doc: &str) -> ApiDocItem {
        ApiDocItem {
            id: Some(id),
            qualified_name: format!("Sample::item{id}"),
            symbol_key: None,
            name: format!("item{id}"),
            kind: "function".to_string(),
            visibility: Some("public".to_string()),
            location: ApiLocation {
                file: "Sample.bd".to_string(),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
            },
            parent_id: parent,
            member_ids: Vec::new(),
            display_name: None,
            module_path: Vec::new(),
            signature: None,
            field_type: None,
            return_type: None,
            parameters: Vec::new(),
            generic_parameters: Vec::new(),
            doc_markdown: Some(doc.to_string()),
            doc: None,
            declaring_package: None,
            controls: vec![],
            tier: None,
        }
    }

    let mut items = vec![
        item(1, None, "/// @tier(standard)"),
        item(2, Some(1), "/// member without directive"),
        item(3, None, "/// @tier(supported)"),
        item(4, None, "/// @tier(unstable)"),
        item(5, None, "/// no tier"),
    ];
    resolve_item_tiers(&mut items);
    assert_eq!(items[0].tier.as_deref(), Some(TIER_STANDARD));
    assert_eq!(items[1].tier.as_deref(), Some(TIER_STANDARD));
    assert_eq!(items[2].tier.as_deref(), Some(TIER_SUPPORTED));
    assert_eq!(items[3].tier.as_deref(), Some(TIER_UNSTABLE));
    assert_eq!(items[4].tier, None);

    let serialized = serde_json::to_string(&items[0]).expect("serialize item");
    assert!(
        serialized.contains("\"tier\":\"standard\""),
        "tier must serialize as camelCase lowercase: {serialized}"
    );
    let omitted = serde_json::to_string(&items[4]).expect("serialize untiered item");
    assert!(
        !omitted.contains("\"tier\""),
        "tier field must be omitted when None: {omitted}"
    );
}

#[test]
fn api_doc_root_advertises_v4_schema_for_tier_metadata() {
    use beskid_analysis::doc::API_JSON_SCHEMA_VERSION;
    assert_eq!(
        API_JSON_SCHEMA_VERSION, 4,
        "tier metadata lives in api.json schema v4; bumping the schema version requires \
         spec sync per ADR D-CORE-API-SHAPE-0003"
    );
}
