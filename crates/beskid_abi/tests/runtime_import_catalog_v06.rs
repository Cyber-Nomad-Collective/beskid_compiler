//! The runtime import catalog has one authority: `runtime_manifest.bsol`.
//!
//! The former ABI-v4 `BUILTIN_SPECS` snapshot (`src/generated/builtins.rs`) claimed to be generated
//! from the manifest, but its generator was removed in the ABI-v5 move and the snapshot drifted
//! (`gc_register_root` returned nothing there while the runtime returns its `u8` status; the
//! composition and callback rows had other arities and results; 14 rows named symbols the manifest
//! no longer declares). These tests regenerate the tables from the manifest source in memory and
//! fail when the compiled catalog differs, and they keep an ungenerated catalog from returning.

use beskid_abi::generated::abi_v5_contract::{ABI_V5_CORELIB_SERVICE_BINDINGS, ABI_V5_SOFT_BUILTINS};
use beskid_abi::runtime_source::{CorelibServiceAbiType, canonical_corelib_service_abi_for_adapter};
use std::path::Path;

fn manifest() -> beskid_manifest::RuntimeManifestV5 {
    let path = beskid_manifest::default_manifest_path(Path::new(env!("CARGO_MANIFEST_DIR")));
    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    beskid_manifest::load_v5_manifest_source(&source).expect("runtime manifest parses")
}

fn service_type(ty: &str) -> CorelibServiceAbiType {
    match ty {
        "pointer" => CorelibServiceAbiType::Pointer,
        "string" => CorelibServiceAbiType::String,
        "usize" | "isize" => CorelibServiceAbiType::Usize,
        "i64" => CorelibServiceAbiType::I64,
        "i32" => CorelibServiceAbiType::I32,
        "u32" => CorelibServiceAbiType::U32,
        "u64" => CorelibServiceAbiType::U64,
        "u8" => CorelibServiceAbiType::U8,
        "f64" => CorelibServiceAbiType::F64,
        "f32" => CorelibServiceAbiType::F32,
        "void" => CorelibServiceAbiType::Void,
        "never" => CorelibServiceAbiType::Never,
        other => panic!("manifest service type `{other}` has no ABI slot"),
    }
}

/// The compiled tables are exactly what the manifest source generates now.
#[test]
fn compiled_abi_v5_tables_match_the_manifest_regenerated_in_memory() {
    let artifacts = beskid_manifest::generate_v5_artifacts(&manifest()).expect("generate ABI-v5 artifacts");
    let compiled = include_str!(concat!(env!("OUT_DIR"), "/abi_v5_contract.rs"));
    assert!(compiled == artifacts.rust, "compiled abi_v5_contract.rs differs from runtime_manifest.bsol; rebuild beskid_abi");
}

/// Every runtime import shape that lowering, the checked-effect path and the object boundary use
/// agrees with the manifest declaration of its Corelib service, on every target.
#[test]
fn every_runtime_service_import_shape_matches_its_manifest_declaration() {
    let manifest = manifest();
    assert!(!manifest.corelib_services.is_empty());
    for service in &manifest.corelib_services {
        let expected_parameters = service.params.iter().map(|parameter| service_type(&parameter.ty)).collect::<Vec<_>>();
        let expected_result = service_type(&service.result);
        for binding in &service.target_bindings {
            let compiled = ABI_V5_CORELIB_SERVICE_BINDINGS
                .iter()
                .filter(|row| row.service == service.name && row.target == binding.target)
                .collect::<Vec<_>>();
            assert_eq!(compiled.len(), 1, "{} on {}: one compiled binding", service.name, binding.target);
            assert_eq!(compiled[0].adapter, service.adapter, "{} adapter", service.name);
            assert_eq!(compiled[0].implementation, binding.implementation, "{} implementation", service.name);
        }
        let abi = canonical_corelib_service_abi_for_adapter(&service.adapter)
            .unwrap_or_else(|| panic!("{} has no unique canonical shape", service.adapter));
        assert_eq!(abi.parameters, expected_parameters, "{} parameters", service.adapter);
        assert_eq!(abi.result, expected_result, "{} result", service.adapter);
        let direct = service.target_bindings.iter().all(|binding| binding.implementation == service.adapter);
        assert_eq!(beskid_abi::is_runtime_import_symbol(&service.adapter), direct, "{} runtime import admission", service.adapter);
    }
}

/// The defect that exposed the drift: the root registration status is a `u8`.
#[test]
fn gc_register_root_returns_its_u8_status() {
    let abi = canonical_corelib_service_abi_for_adapter("gc_register_root").expect("gc_register_root shape");
    assert_eq!(abi.parameters, vec![CorelibServiceAbiType::Pointer]);
    assert_eq!(abi.result, CorelibServiceAbiType::U8);
    assert!(beskid_abi::is_runtime_import_symbol("gc_register_root"));
}

/// The builtin catalog is the manifest soft builtins and nothing else, and no checked-in catalog
/// without a generator exists beside the generated tables.
#[test]
fn builtin_catalog_is_only_the_generated_soft_builtins() {
    let manifest = manifest();
    let mut expected = manifest.soft_builtins.iter().map(|builtin| builtin.symbol.as_str()).collect::<Vec<_>>();
    let mut compiled = beskid_abi::all_builtin_specs().map(|spec| spec.symbol).collect::<Vec<_>>();
    expected.sort_unstable();
    compiled.sort_unstable();
    assert_eq!(compiled, expected);
    assert_eq!(ABI_V5_SOFT_BUILTINS.len(), manifest.soft_builtins.len());
    for retired in ["dynamic_map_aot", "composition_register", "panic", "panic_str", "beskid_runtime_abi_version", "fiber_yield"] {
        assert!(!beskid_abi::is_runtime_import_symbol(retired), "{retired} has no ABI-v5 manifest authority");
    }
    let generated = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/generated");
    let mut files = std::fs::read_dir(&generated)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files, ["abi_v5_contract.rs", "mod.rs", "symbols.rs"], "unexpected file in {}", generated.display());
}
