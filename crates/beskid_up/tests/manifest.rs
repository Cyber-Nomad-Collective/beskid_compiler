use beskid_up::ReleaseManifest;

#[test]
fn selects_a_bundle_for_an_exact_target() {
    let manifest = ReleaseManifest::from_json(
        r#"{
          "schema": 1,
          "version": "1.2.3",
          "bundles": [{
            "target": "x86_64-unknown-linux-gnu",
            "url": "https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/v1.2.3/beskid.tar.gz",
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          }]
        }"#,
    )
    .unwrap();

    assert_eq!(
        manifest.select_bundle("x86_64-unknown-linux-gnu").unwrap().url,
        "https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/v1.2.3/beskid.tar.gz"
    );
}

#[test]
fn rejects_non_https_bundle_urls() {
    let result = ReleaseManifest::from_json(
        r#"{
          "schema": 1,
          "version": "1.2.3",
          "bundles": [{
            "target": "x86_64-unknown-linux-gnu",
            "url": "http://example.invalid/beskid.tar.gz",
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          }]
        }"#,
    );

    assert!(result.is_err());
}
