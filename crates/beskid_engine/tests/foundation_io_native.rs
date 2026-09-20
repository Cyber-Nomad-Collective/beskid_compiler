#![cfg(unix)]

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use std::{path::Path, sync::Arc};

#[test]
fn foundation_utf8_rejects_non_scalar_and_non_shortest_sequences() {
    run_foundation_fixture("foundation_utf8.bd");
}

#[test]
fn foundation_io_contract_dispatches_reader_implementation() {
    run_foundation_fixture("foundation_contract_dispatch.bd");
}

#[test]
fn foundation_contract_witnesses_preserve_distinct_receivers_through_forwarding_and_collection() {
    run_foundation_fixture("foundation_contract_witnesses.bd");
}

#[test]
fn foundation_base64_validates_each_quantum_and_preserves_prior_quanta() {
    run_foundation_fixture("foundation_base64.bd");
}

#[test]
fn foundation_utf8_materializes_every_accepted_code_unit() {
    run_foundation_fixture("foundation_utf8_text.bd");
}

#[test]
fn foundation_utf8_does_not_expose_lossy_append_route() {
    let result = lower_foundation_fixture("foundation_utf8_legacy_append.bd");
    assert!(result.is_err(), "the deprecated UTF-8 append route must not be a public callable");
}

#[test]
fn foundation_string_does_not_expose_lossy_utf8_append_route() {
    let result = lower_foundation_fixture("foundation_string_legacy_append.bd");
    assert!(result.is_err(), "the Core.String forwarding append route must not be a public callable");
}

#[test]
fn foundation_byte_cursors_validate_ranges_and_preserve_position_on_failure() {
    run_foundation_fixture("foundation_bytes.bd");
}

#[test]
fn foundation_http_ascii_and_hex_decode_strictly() {
    run_foundation_fixture("foundation_ascii_hex.bd");
}

#[test]
fn foundation_copy_rejects_every_invalid_range_before_mutation() {
    run_foundation_fixture("foundation_copy.bd");
}

#[test]
// F6 remains RED at IO.bd:32: block-arm enum construction loses contextual Result arguments.
// Static contract specialization and direct array-literal argument admission now pass.
fn foundation_io_transfers_validate_ranges_and_handle_partial_eof_and_progress() {
    run_foundation_fixture("foundation_io.bd");
}

#[test]
fn foundation_syscall_read_bytes_with_preserves_bytes_result() {
    use beskid_analysis::{
        syntax::{FunctionDefinition, PrimitiveType, Type},
        syntax_query::{NodeKind, SyntaxIndex},
    };
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/foundation/src/Core/Syscall/Syscall.bd");
    let source = std::fs::read_to_string(&path).unwrap();
    let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
    let index = SyntaxIndex::from_program(&program, SyntaxGenerationId(96));
    let definition = index
        .ids_of_kind(NodeKind::FunctionDefinition)
        .filter_map(|id| index.node_at(&program, id)?.of::<FunctionDefinition>())
        .find(|function| function.name.node.name == "ReadBytesWith")
        .unwrap();
    let Type::Complex(path) = &definition.return_type.as_ref().unwrap().node else {
        panic!("Result return type");
    };
    let result = path.node.segments.last().unwrap();
    assert!(
        matches!(&result.node.type_args[0].node, Type::Array(element) if matches!(element.node, Type::Primitive(ref ty) if ty.node == PrimitiveType::U8)),
        "ReadBytesWith must publicly declare a byte-array success payload"
    );
    run_foundation_fixture("foundation_syscall.bd");
}

#[test]
fn foundation_numeric_panic_preserves_process_trap_code() {
    let compiler = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let prefix = tempfile::tempdir().unwrap();
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).unwrap();
    // Keep every canonical runtime object; replace only the terminal host trap for observation.
    let observed_library = prefix.path().join("observed-runtime.a");
    std::fs::copy(&kit.static_library, &observed_library).unwrap();
    assert!(
        std::process::Command::new("ar")
            .arg("d")
            .arg(&observed_library)
            .arg("beskid_runtime.platform_host.o")
            .status()
            .unwrap()
            .success()
    );
    let target = beskid_engine::host_runtime_target().unwrap();
    let adapter = prefix.path().join("observed-platform.o");
    let output = std::process::Command::new("cc")
        .arg("-std=c11")
        .arg("-c")
        .arg("-Dbeskid_rt_v5_intrinsic_trap=original_platform_trap")
        .arg(compiler.join("crates/beskid_abi/assembly").join(target.triple.as_str()).join("platform_host.c"))
        .arg("-o")
        .arg(&adapter)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for code in [2, 3, 5] {
        let executable = prefix.path().join(format!("panic-{code}"));
        let output = std::process::Command::new("cc")
            .arg("-std=c11")
            .arg("-I")
            .arg(compiler.join("crates/beskid_abi/include"))
            .arg(format!("-DPANIC_CODE={code}"))
            .arg(compiler.join("crates/beskid_engine/tests/fixtures/foundation_panic.c"))
            .arg(&observed_library)
            .arg(&adapter)
            .args(["-lpthread", "-lm", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let output = std::process::Command::new(&executable).output().unwrap();
        assert_eq!(output.status.code(), Some(101));
        assert_eq!(output.stdout, b"observed-trap\n", "test must observe the actual trap boundary");
    }
    for (mode, library) in [("aot", &kit.static_library), ("native-kit", &kit.shared_library)] {
        for code in [2, 5] {
            let executable = prefix.path().join(format!("child-{mode}-{code}"));
            let output = std::process::Command::new("cc")
                .args(["-std=c11", "-DCHILD_PANIC=1", "-I"])
                .arg(compiler.join("crates/beskid_abi/include"))
                .arg(format!("-DPANIC_CODE={code}"))
                .arg(compiler.join("crates/beskid_engine/tests/fixtures/foundation_panic.c"))
                .arg(library)
                .args(["-lpthread", "-lm", "-o"])
                .arg(&executable)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let output = std::process::Command::new(&executable)
                .env(
                    if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                    kit.shared_library.parent().unwrap(),
                )
                .output()
                .unwrap();
            assert!(output.status.success(), "child panic {mode} {code}: {output:?}");
        }
    }
}

fn lower_foundation_fixture(fixture: &str) -> anyhow::Result<beskid_codegen::PreparedSyntaxEntrypoint> {
    let compiler = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let foundation = compiler.join("corelib/packages/foundation/src");
    let concurrency = compiler.join("corelib/packages/concurrency/src");
    let fixtures = compiler.join("crates/beskid_engine/tests/fixtures");
    let mut paths = vec![(fixtures.join(fixture), fixture.to_owned())];
    if fixture == "foundation_contract_witnesses.bd" {
        for relative in ["Concurrency/Fiber.bd", "Concurrency/FiberError.bd"] {
            paths.push((concurrency.join(relative), relative.to_owned()));
        }
    }
    for relative in [
        "Core/Results/Results.bd",
        "Core/Bytes/Slice.bd",
        "Core/Bytes/Convert.bd",
        "Core/Collections/Array.bd",
        "Core/Collections/Array/ArrayIter.bd",
        "Core/Encoding/EncodingError.bd",
        "Core/Encoding/Utf8.bd",
        "Core/Encoding/Hex.bd",
        "Core/Encoding/Base64.bd",
        "Core/Encoding/Encoding.bd",
        "Core/Encoding/Contract.bd",
        "Core/String/String.bd",
        "Core/String/Core.bd",
        "Core/String/Chars.bd",
        "Core/String/Utf8.bd",
    ] {
        paths.push((foundation.join(relative), relative.to_owned()));
    }
    if fixture == "foundation_bytes.bd" {
        for relative in [
            "Core/Bytes/Errors.bd",
            "Core/Bytes/ByteReader.bd",
            "Core/Bytes/ByteWriter.bd",
            "Core/Bytes/BufferReader.bd",
            "Core/Bytes/BufferWriter.bd",
        ] {
            paths.push((foundation.join(relative), relative.to_owned()));
        }
    }
    if fixture == "foundation_syscall.bd" {
        for file in [
            "Syscall",
            "SyscallError",
            "Descriptor",
            "StandardStream",
            "ReadLimit",
            "ReadRequest",
            "ReadBytesRequest",
            "WriteRequest",
            "WriteBytesRequest",
        ] {
            let relative = format!("Core/Syscall/{file}.bd");
            paths.push((foundation.join(&relative), relative));
        }
    }
    if fixture == "foundation_io.bd" {
        for file in ["IO", "Reader", "Writer", "Closer", "Stream", "IoError"] {
            let relative = format!("Core/IO/{file}.bd");
            paths.push((foundation.join(&relative), relative));
        }
        paths.push((foundation.join("Core/Disposable.bd"), "Core/Disposable.bd".into()));
    }
    let units = paths
        .into_iter()
        .map(|(path, logical_name)| {
            let source = std::fs::read_to_string(&path).unwrap();
            let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
            SourceUnit { path, logical_name, source, program }
        })
        .collect();
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: fixtures },
            dependencies: vec![
                RootEntry { dependency_name: Some("foundation".into()), source_root: foundation },
                RootEntry { dependency_name: Some("concurrency".into()), source_root: concurrency },
            ],
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(96),
    ));
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    beskid_codegen::lower_syntax_assembly_entrypoint(
        &mut BeskidDatabase::default(),
        assembly,
        "RunFoundationFixture",
        target.clone(),
        isa.as_ref(),
    )
}

fn run_foundation_fixture(fixture: &str) {
    let compiler = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let target = beskid_engine::host_runtime_target().unwrap();
    let lowered = lower_foundation_fixture(fixture).expect("Foundation fixture source lowering");
    eprintln!("{fixture}: source lowering complete");
    let prefix = tempfile::tempdir().unwrap();
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).unwrap();
    eprintln!("{fixture}: native kit complete");
    if fixture == "foundation_copy.bd" {
        let object_path = prefix.path().join("copy.o");
        let symbol = beskid_codegen::object_link_symbol(&lowered.symbol, &lowered.artifact.exports);
        let mut object =
            beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).unwrap();
        object
            .compile_artifact_with_exports(&lowered.artifact, &std::collections::HashSet::from([symbol.clone()]), None)
            .unwrap();
        object.finalize_to_path(&object_path).unwrap();
        let observed = prefix.path().join("observed-copy.a");
        std::fs::copy(&kit.static_library, &observed).unwrap();
        assert!(
            std::process::Command::new("ar")
                .arg("d")
                .arg(&observed)
                .arg("beskid_runtime.platform_host.o")
                .status()
                .unwrap()
                .success()
        );
        let adapter = prefix.path().join("copy-platform.o");
        let output = std::process::Command::new("cc")
            .args(["-std=c11", "-c", "-Dbeskid_rt_v5_intrinsic_trap=original_platform_trap"])
            .arg(compiler.join("crates/beskid_abi/assembly").join(target.triple.as_str()).join("platform_host.c"))
            .arg("-o")
            .arg(&adapter)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        for case in 0..10 {
            let executable = prefix.path().join(format!("copy-{case}"));
            let output = std::process::Command::new("cc")
                .arg("-std=c11")
                .arg("-I")
                .arg(compiler.join("crates/beskid_abi/include"))
                .arg(format!(
                    "-DFOUNDATION_FIXTURE_SYMBOL=\"{}{}\"",
                    if cfg!(target_os = "macos") { "_" } else { "" },
                    symbol
                ))
                .arg(format!("-DCOPY_CASE={case}"))
                .arg(compiler.join("crates/beskid_engine/tests/fixtures/foundation_copy.c"))
                .arg(&object_path)
                .arg(&observed)
                .arg(&adapter)
                .args(["-lpthread", "-lm", "-o"])
                .arg(&executable)
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let output = std::process::Command::new(executable).output().unwrap();
            assert_eq!(output.status.code(), Some(if case == 6 { 0 } else { 101 }), "case {case}: {output:?}");
            if case != 6 {
                assert_eq!(output.stdout, b"unchanged-bounds\n");
            }
        }
        return;
    }
    let mut engine = beskid_engine::Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).unwrap();
    engine.compile_artifact(&lowered.artifact).expect("Foundation JIT");
    let entry = unsafe { engine.entrypoint_ptr(&lowered.symbol) }.unwrap();
    let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(entry) };
    if fixture == "foundation_syscall.bd" {
        let mut descriptors = [-1; 2];
        unsafe {
            assert_eq!(libc::fcntl(198, libc::F_GETFD), -1, "test descriptor must be unused");
            assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::EBADF));
            assert_eq!(libc::pipe(descriptors.as_mut_ptr()), 0);
            assert!(!descriptors.contains(&198), "reserved fixture descriptor");
            assert_eq!(libc::write(descriptors[1], [0_u8, 255, 42].as_ptr().cast(), 3), 3);
            assert_eq!(libc::dup2(descriptors[0], 198), 198);
            libc::close(descriptors[0]);
            libc::close(descriptors[1]);
        }
    }
    assert_eq!(run(), 42, "{fixture}: Foundation behavior failure code");
    if fixture == "foundation_syscall.bd" {
        unsafe {
            libc::close(198);
        }
    }
    drop(engine);
    let object_path = prefix.path().join("foundation.o");
    let native_symbol = beskid_codegen::object_link_symbol(&lowered.symbol, &lowered.artifact.exports);
    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).unwrap();
    object
        .compile_artifact_with_exports(
            &lowered.artifact,
            &std::collections::HashSet::from([native_symbol.clone()]),
            None,
        )
        .unwrap();
    object.finalize_to_path(&object_path).unwrap();
    for (mode, library) in [("aot", &kit.static_library), ("native-kit", &kit.shared_library)] {
        let executable = prefix.path().join(mode);
        let output = std::process::Command::new("cc")
            .arg("-std=c11")
            .arg("-I")
            .arg(compiler.join("crates/beskid_abi/include"))
            .arg(format!("-DSYSCALL_INPUT={}", i32::from(fixture == "foundation_syscall.bd")))
            .arg(format!(
                "-DFOUNDATION_FIXTURE_SYMBOL=\"{}{}\"",
                if cfg!(target_os = "macos") { "_" } else { "" },
                native_symbol
            ))
            .arg(compiler.join("crates/beskid_engine/tests/fixtures/foundation_io.c"))
            .arg(&object_path)
            .arg(library)
            .args(["-lpthread", "-lm", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(output.status.success(), "{fixture} {mode}: {}", String::from_utf8_lossy(&output.stderr));
        let output = std::process::Command::new(executable)
            .env(
                if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                kit.shared_library.parent().unwrap(),
            )
            .output()
            .unwrap();
        assert!(output.status.success(), "{fixture} {mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
