use beskid_manifest::{generate_v5_artifacts, load_v5_manifest_source};

#[test]
fn traced_value_layout_and_owner_operations_cross_every_generated_boundary() {
    let manifest = load_v5_manifest_source(include_str!("../../../runtime_manifest.bsol")).unwrap();
    let layout = manifest
        .layouts
        .iter()
        .find(|layout| layout.name == "BeskidAbiValue")
        .expect("Foundation requires the canonical traced BeskidAbiValue layout");
    assert_eq!((layout.size, layout.alignment), (40, 8));
    assert_eq!(
        layout.fields.iter().map(|field| (field.name.as_str(), field.offset, field.ty.as_str())).collect::<Vec<_>>(),
        [
            ("tag", 0, "usize"),
            ("payload", 8, "pointer"),
            ("descriptor", 16, "pointer"),
            ("owner_heap", 24, "pointer"),
            ("owner_state", 32, "usize")
        ]
    );
    for operation in ["initialize", "replace_with_barrier", "move_out", "clear"] {
        let symbol = format!("beskid_rt_v5_abi_value_{operation}");
        assert!(manifest.exports.iter().any(|export| export.symbol == symbol), "missing {symbol}");
    }
    let generated = generate_v5_artifacts(&manifest).unwrap();
    assert!(generated.c_header.contains("#define BESKID_ABI_VALUE_PAYLOAD_OFFSET 8"));
    assert!(generated.rust.contains("BESKID_ABI_VALUE_PAYLOAD_OFFSET = 8"));
    assert!(generated.abi_json.contains("BeskidAbiValue"));
}
