use beskid_manifest::load_v5_manifest_source;

// This audit intentionally becomes mandatory only after F2/F3 (Tasks 3 and 4) migrate
// Fiber and Channel together. Keeping the assertion executable prevents a prose-only gate.
fn violations<'a>(entries: impl IntoIterator<Item = (&'a str, Vec<&'a str>, &'a str)>) -> Vec<String> {
    entries
        .into_iter()
        .filter_map(|(name, params, result)| {
            let transport = name.contains("fiber_join_value")
                || name.contains("channel_send")
                || name.contains("channel_try_send")
                || name.contains("channel_recv")
                || name.contains("channel_receive");
            if !transport {
                return None;
            }
            let side_channel = name.ends_with("_ptr") || name.ends_with("_pointer") || name.ends_with("_scalar");
            let value_slot = params.iter().skip(1).any(|ty| *ty == "pointer");
            let scalar_value =
                (name.contains("fiber_join_value") || name.contains("channel_receive_value")) && !value_slot;
            let scalar_send = name.contains("send") && params.get(1) != Some(&"pointer");
            let pointer_return = result == "pointer";
            (side_channel || scalar_value || scalar_send || pointer_return).then(|| name.to_owned())
        })
        .collect()
}

#[test]
fn audit_rejects_scalar_and_pointer_side_channels_but_accepts_one_slot_protocol() {
    assert_eq!(
        violations([
            ("__fiber_join_value", vec!["i64"], "i64"),
            ("__channel_send", vec!["i64", "i64"], "i64"),
            ("__channel_send_ptr", vec!["i64", "pointer"], "i64"),
            ("__channel_recv_ptr", vec!["i64"], "pointer"),
            ("__channel_receive_value", vec!["i64"], "i64"),
        ]),
        ["__fiber_join_value", "__channel_send", "__channel_send_ptr", "__channel_recv_ptr", "__channel_receive_value"]
    );
    assert!(
        violations([
            ("__fiber_join_value", vec!["i64", "pointer"], "u8"),
            ("__channel_send", vec!["i64", "pointer"], "i64"),
            ("__channel_receive", vec!["i64", "pointer"], "i64"),
        ])
        .is_empty()
    );
}

#[test]
#[ignore = "F1 staging gate: enable after Tasks 3 (Fiber) and 4 (Channel) migrate all consumers"]
fn public_foundation_transport_has_no_scalar_or_pointer_side_channel() {
    let manifest = load_v5_manifest_source(include_str!("../../../runtime_manifest.bsol")).unwrap();
    let entries = manifest
        .exports
        .iter()
        .map(|entry| {
            (entry.symbol.as_str(), entry.params.iter().map(|param| param.ty.as_str()).collect(), entry.result.as_str())
        })
        .chain(manifest.soft_builtins.iter().map(|entry| {
            (entry.name.as_str(), entry.params.iter().map(|param| param.ty.as_str()).collect(), entry.result.as_str())
        }))
        .chain(manifest.corelib_services.iter().map(|entry| {
            (entry.name.as_str(), entry.params.iter().map(|param| param.ty.as_str()).collect(), entry.result.as_str())
        }))
        .chain(manifest.soft_builtins.iter().map(|entry| {
            (entry.symbol.as_str(), entry.params.iter().map(|param| param.ty.as_str()).collect(), entry.result.as_str())
        }))
        .chain(manifest.corelib_services.iter().map(|entry| {
            (
                entry.adapter.as_str(),
                entry.params.iter().map(|param| param.ty.as_str()).collect(),
                entry.result.as_str(),
            )
        }));
    assert_eq!(violations(entries), Vec::<String>::new(), "F2/F3 transport migration is incomplete");
}
