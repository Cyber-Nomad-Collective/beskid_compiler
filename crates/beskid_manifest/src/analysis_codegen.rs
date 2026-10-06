use std::fmt::Write as _;

use crate::v5::RuntimeManifestV5;

pub fn append_analysis_intrinsics(base: &str, runtime: &RuntimeManifestV5) -> Result<String, String> {
    const MARKER: &str = "// ABI-v5 canonical runtime declarations";
    const OLD_MARKER: &str = "// ABI-v5 canonical runtime intrinsic candidates";
    let generated_start = [base.find(MARKER), base.find(OLD_MARKER)].into_iter().flatten().min().unwrap_or(base.len());
    // The baseline contains source-only facades as well as historical copies of
    // manifest-owned services. Keep facades; regenerate every owned declaration.
    let mut out = String::new();
    let mut remaining = &base[..generated_start];
    while let Some(start) = remaining.find("    &[\"") {
        out.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let name_end = remaining[7..].find('"').ok_or("malformed builtin baseline path")? + 7;
        let name = &remaining[7..name_end];
        let end = remaining.find("    },").ok_or("malformed builtin baseline entry")? + 6;
        if !runtime.corelib_services.iter().any(|service| service.name == name) {
            out.push_str(&remaining[..end]);
        }
        remaining = &remaining[end..];
    }
    out.push_str(remaining);
    if !out.trim_end().ends_with('}') {
        out.push_str("}\n");
    }
    let closing = out.rfind('}').expect("generated builtins closes macro");
    let mut entries = format!("{MARKER}\n");
    for service in &runtime.corelib_services {
        if let Some(facade) = runtime.soft_builtins.iter().find(|builtin| builtin.name == service.name) {
            if facade.adapter_service.as_deref() != Some(service.name.as_str()) || facade.symbol != service.adapter {
                return Err(format!("conflicting source builtin and Corelib service: {}", service.name));
            }
            // The explicitly declared source facade owns this callable name. Its
            // managed source representation differs from the raw service ABI.
            continue;
        }
        if TYPED_VALUE_SERVICES.contains(&service.name.as_str()) {
            let transport = service.params.iter().map(|parameter| parameter.ty.as_str()).collect::<Vec<_>>();
            write_typed_value_entry(&mut entries, &service.name, &service.adapter, &transport, &service.result)?;
            continue;
        }
        if let Some(managed) = managed_source_service(&service.name) {
            let transport = service.params.iter().map(|parameter| parameter.ty.as_str()).collect::<Vec<_>>();
            write_managed_source_entry(&mut entries, managed, &service.adapter, &transport, &service.result)?;
            continue;
        }
        let params = service.params.iter().map(|parameter| analysis_type(&parameter.ty)).collect::<Vec<_>>();
        write_entry(&mut entries, &service.name, &service.adapter, &params, &analysis_type(&service.result))?;
    }
    for intrinsic in &runtime.intrinsics {
        let params = intrinsic.params.iter().map(|parameter| analysis_type(&parameter.ty)).collect::<Vec<_>>();
        write_entry(&mut entries, &intrinsic.name, &intrinsic.symbol, &params, &analysis_type(&intrinsic.result))?;
    }
    for builtin in &runtime.soft_builtins {
        let params = builtin.params.iter().map(|parameter| analysis_type(&parameter.ty)).collect::<Vec<_>>();
        write_entry(&mut entries, &builtin.name, &builtin.symbol, &params, &analysis_type(&builtin.result))?;
    }
    out.insert_str(closing, &entries);
    Ok(out)
}

/// Source-authorized typed value services (`beskid_abi::canonical_corelib_service_value_dispatch`).
/// Their manifest row is the traced caller-slot transport `(handle: i64, destination: pointer) -> u8`;
/// the compiler owns the destination slot, so the source callable is `T name<T>(i64 handle)`.
const TYPED_VALUE_SERVICES: &[&str] = &["__fiber_join_value", "__channel_receive_value", "__hub_wait_receive_value"];

fn write_typed_value_entry(
    out: &mut String,
    name: &str,
    adapter: &str,
    transport: &[&str],
    result: &str,
) -> Result<(), String> {
    if transport != ["i64", "pointer"] || result != "u8" {
        return Err(format!(
            "typed value service {name} must declare the traced caller-slot transport (i64, pointer) -> u8"
        ));
    }
    writeln!(out, "    &[\"{name}\"] => {{").unwrap();
    writeln!(out, "        symbol: \"{adapter}\",").unwrap();
    writeln!(out, "        type_parameters: [\"T\"],").unwrap();
    writeln!(out, "        params: [I64],").unwrap();
    writeln!(out, "        returns: TypeParameter,").unwrap();
    writeln!(out, "        injected: true,").unwrap();
    writeln!(out, "    }},").unwrap();
    Ok(())
}

/// Source-authorized Corelib services whose source callable takes or returns managed values
/// while the manifest row declares the raw native transport. Codegen derives the transport from
/// the managed header at the call boundary (`beskid_isle` corelib service adaptation) or lowers
/// the typed allocation form directly, so the analysis table carries the source signature.
struct ManagedSourceService {
    name: &'static str,
    transport: &'static [&'static str],
    transport_result: &'static str,
    type_parameters: &'static [&'static str],
    params: &'static [&'static str],
    returns: &'static str,
}

const MANAGED_SOURCE_SERVICES: &[ManagedSourceService] = &[
    // `string __str_from_bytes_utf8(u8[] bytes)`: the header supplies `(data, len)`.
    ManagedSourceService {
        name: "__str_from_bytes_utf8",
        transport: &["pointer", "usize"],
        transport_result: "pointer",
        type_parameters: &[],
        params: &["Bytes"],
        returns: "String",
    },
    // `i64 __syscall_write_bytes(i32 descriptor, u8[] bytes)`: the header supplies `(data, len)`.
    ManagedSourceService {
        name: "__syscall_write_bytes",
        transport: &["i32", "pointer", "usize"],
        transport_result: "i64",
        type_parameters: &[],
        params: &["I32", "Bytes"],
        returns: "I64",
    },
    // `T[] __array_new<T>(usize count)`: the element descriptor comes from the specialized `T`.
    ManagedSourceService {
        name: "__array_new",
        transport: &["usize", "usize"],
        transport_result: "pointer",
        type_parameters: &["T"],
        params: &["Usize"],
        returns: "TypeParameterArray",
    },
];

fn managed_source_service(name: &str) -> Option<&'static ManagedSourceService> {
    MANAGED_SOURCE_SERVICES.iter().find(|service| service.name == name)
}

fn write_managed_source_entry(
    out: &mut String,
    service: &ManagedSourceService,
    adapter: &str,
    transport: &[&str],
    result: &str,
) -> Result<(), String> {
    if transport != service.transport || result != service.transport_result {
        return Err(format!(
            "managed source service {} must declare the native transport ({}) -> {}",
            service.name,
            service.transport.join(", "),
            service.transport_result
        ));
    }
    writeln!(out, "    &[\"{}\"] => {{", service.name).unwrap();
    writeln!(out, "        symbol: \"{adapter}\",").unwrap();
    if !service.type_parameters.is_empty() {
        let parameters =
            service.type_parameters.iter().map(|parameter| format!("\"{parameter}\"")).collect::<Vec<_>>().join(", ");
        writeln!(out, "        type_parameters: [{parameters}],").unwrap();
    }
    writeln!(out, "        params: [{}],", service.params.join(", ")).unwrap();
    writeln!(out, "        returns: {},", service.returns).unwrap();
    writeln!(out, "        injected: true,").unwrap();
    writeln!(out, "    }},").unwrap();
    Ok(())
}

fn write_entry(out: &mut String, name: &str, symbol: &str, params: &[String], result: &str) -> Result<(), String> {
    let params = params.iter().map(|param| type_ident(param)).collect::<Result<Vec<_>, _>>()?.join(", ");
    writeln!(out, "    &[\"{name}\"] => {{").unwrap();
    writeln!(out, "        symbol: \"{symbol}\",").unwrap();
    writeln!(out, "        params: [{params}],").unwrap();
    writeln!(out, "        returns: {},", type_ident(result)?).unwrap();
    writeln!(out, "        injected: true,").unwrap();
    writeln!(out, "    }},").unwrap();
    Ok(())
}

fn analysis_type(ty: &str) -> String {
    match ty {
        "pointer" => "ptr".into(),
        "void" => "unit".into(),
        other => other.into(),
    }
}

fn type_ident(ty: &str) -> Result<&'static str, String> {
    Ok(match ty {
        "string" => "String",
        "ptr" => "Ptr",
        "usize" => "Usize",
        "isize" => "Isize",
        "i8" => "I8",
        "i16" => "I16",
        "i32" => "I32",
        "i64" => "I64",
        "u8" => "U8",
        "u16" => "U16",
        "u32" => "U32",
        "u64" => "U64",
        "f32" => "F32",
        "f64" => "F64",
        "bool" => "Bool",
        "char" => "Char",
        "unit" | "void" => "Unit",
        "never" => "Never",
        _ => return Err(format!("unsupported analysis builtin representation: {ty}")),
    })
}

#[cfg(test)]
mod tests {
    use super::append_analysis_intrinsics;

    #[test]
    fn analysis_builtin_representation_preserves_all_fixed_scalar_types() {
        for (source, expected) in [
            ("i8", "I8"),
            ("i16", "I16"),
            ("i32", "I32"),
            ("i64", "I64"),
            ("u8", "U8"),
            ("u16", "U16"),
            ("u32", "U32"),
            ("u64", "U64"),
            ("f32", "F32"),
            ("f64", "F64"),
            ("usize", "Usize"),
            ("isize", "Isize"),
        ] {
            assert_eq!(super::type_ident(&super::analysis_type(source)).unwrap(), expected, "{source}");
        }
    }

    #[test]
    fn unknown_builtin_type_is_rejected_instead_of_unit() {
        assert!(super::type_ident("future_scalar").is_err());
    }

    #[test]
    fn every_source_service_overlap_declares_its_canonical_adapter_binding() {
        let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime_manifest.bsol"));
        let runtime = crate::load_v5_manifest_source(source).unwrap();
        for facade in &runtime.soft_builtins {
            if let Some(service) = runtime.corelib_services.iter().find(|service| service.name == facade.name) {
                assert_eq!(
                    facade.adapter_service.as_deref(),
                    Some(service.name.as_str()),
                    "{} requires explicit binding",
                    facade.name
                );
                assert_eq!(facade.symbol, service.adapter, "{} requires canonical adapter symbol", facade.name);
            }
        }
    }

    #[test]
    fn every_manifest_corelib_service_has_one_exact_generated_signature() {
        let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime_manifest.bsol"));
        let runtime = crate::load_v5_manifest_source(source).unwrap();
        let baseline =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../beskid_analysis/src/generated/builtins.inc.rs"));
        let generated = append_analysis_intrinsics(baseline, &runtime).unwrap();
        for service in &runtime.corelib_services {
            let marker = format!("&[\"{}\"] =>", service.name);
            assert_eq!(generated.matches(&marker).count(), 1, "{} must have one authority", service.name);
            if super::TYPED_VALUE_SERVICES.contains(&service.name.as_str()) {
                let mut expected = String::new();
                let transport = service.params.iter().map(|parameter| parameter.ty.as_str()).collect::<Vec<_>>();
                super::write_typed_value_entry(&mut expected, &service.name, &service.adapter, &transport, &service.result)
                    .unwrap();
                assert!(generated.contains(&expected), "{} generic source signature missing", service.name);
                assert!(expected.contains("params: [I64],") && expected.contains("returns: TypeParameter,"));
                continue;
            }
            if let Some(managed) = super::managed_source_service(&service.name) {
                let mut expected = String::new();
                let transport = service.params.iter().map(|parameter| parameter.ty.as_str()).collect::<Vec<_>>();
                super::write_managed_source_entry(&mut expected, managed, &service.adapter, &transport, &service.result)
                    .unwrap();
                assert!(generated.contains(&expected), "{} managed source signature missing", service.name);
                continue;
            }
            let facade = runtime.soft_builtins.iter().find(|builtin| builtin.name == service.name);
            let (parameters, result) = if let Some(facade) = facade {
                assert_eq!(facade.adapter_service.as_deref(), Some(service.name.as_str()));
                assert_eq!(facade.symbol, service.adapter);
                (&facade.params, &facade.result)
            } else {
                (&service.params, &service.result)
            };
            let params = parameters.iter().map(|parameter| super::analysis_type(&parameter.ty)).collect::<Vec<_>>();
            let mut expected = String::new();
            super::write_entry(&mut expected, &service.name, &service.adapter, &params, &super::analysis_type(result))
                .unwrap();
            assert!(generated.contains(&expected), "{} exact signature missing", service.name);
        }
        for facade in ["range", "__channel_receive", "__hub_wait_receive", "__test_bytes_len", "__test_bytes_ptr"] {
            assert_eq!(
                generated.matches(&format!("&[\"{facade}\"] =>")).count(),
                1,
                "explicit source facade remains distinct"
            );
        }
        let mut scalar_value = runtime.clone();
        let service =
            scalar_value.corelib_services.iter_mut().find(|service| service.name == "__channel_receive_value").unwrap();
        service.params.truncate(1);
        service.result = "i64".into();
        assert!(
            append_analysis_intrinsics(baseline, &scalar_value).is_err(),
            "a typed value service without the caller-slot transport rejects"
        );
        let mut conflicting = runtime.clone();
        conflicting.soft_builtins.iter_mut().find(|builtin| builtin.name == "__str_len").unwrap().adapter_service =
            None;
        assert!(
            append_analysis_intrinsics(baseline, &conflicting).is_err(),
            "same name without declared adapter ownership rejects"
        );
    }

    #[test]
    fn managed_source_services_publish_source_signatures_and_reject_transport_drift() {
        let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime_manifest.bsol"));
        let runtime = crate::load_v5_manifest_source(source).unwrap();
        let baseline =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../beskid_analysis/src/generated/builtins.inc.rs"));
        let generated = append_analysis_intrinsics(baseline, &runtime).unwrap();
        for (name, signature) in [
            ("__str_from_bytes_utf8", "        params: [Bytes],\n        returns: String,\n"),
            ("__syscall_write_bytes", "        params: [I32, Bytes],\n        returns: I64,\n"),
            (
                "__array_new",
                "        type_parameters: [\"T\"],\n        params: [Usize],\n        returns: TypeParameterArray,\n",
            ),
        ] {
            let start = generated.find(&format!("&[\"{name}\"] =>")).expect("managed service entry");
            let entry = &generated[start..start + generated[start..].find("    },").expect("entry end")];
            assert!(entry.contains(signature), "{name} source signature: {entry}");
        }

        let mut drifted = runtime.clone();
        drifted.corelib_services.iter_mut().find(|service| service.name == "__syscall_write_bytes").unwrap().params
            .truncate(2);
        assert!(
            append_analysis_intrinsics(baseline, &drifted).is_err(),
            "a managed source service whose native transport changed fails closed"
        );
    }

    #[test]
    fn generated_builtin_block_is_idempotent_with_crlf_input() {
        let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime_manifest.bsol"));
        let runtime = crate::load_v5_manifest_source(source).expect("canonical runtime manifest");
        let checked_in =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../beskid_analysis/src/generated/builtins.inc.rs"));
        let crlf = checked_in.replace('\n', "\r\n");

        let once = append_analysis_intrinsics(&crlf, &runtime).unwrap();
        let twice = append_analysis_intrinsics(&once, &runtime).unwrap();

        assert_eq!(once.matches("// ABI-v5 canonical runtime declarations").count(), 1);
        assert_eq!(once.matches("&[\"native_word_from_pointer\"]").count(), 1);
        assert_eq!(twice, once);
    }
}
