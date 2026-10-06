use beskid_up::ReleaseManifest;

#[test]
fn selects_a_bundle_for_an_exact_target() {
    let manifest = ReleaseManifest::from_json(
        r#"{
          "schema": 1,
          "version": "1.2.3",
          "commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "bundles": [{
            "target": "x86_64-unknown-linux-gnu",
            "url": "https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/cli-v1.2.3/beskid.tar.gz",
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          }]
        }"#,
    )
    .unwrap();

    assert_eq!(
        manifest.select_bundle("x86_64-unknown-linux-gnu").unwrap().url,
        "https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/cli-v1.2.3/beskid.tar.gz"
    );
}

#[test]
fn rejects_non_https_bundle_urls() {
    let result = ReleaseManifest::from_json(
        r#"{
          "schema": 1,
          "version": "1.2.3",
          "commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "bundles": [{
            "target": "x86_64-unknown-linux-gnu",
            "url": "http://example.invalid/beskid.tar.gz",
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          }]
        }"#,
    );

    assert!(result.is_err());
}

#[test]
fn v06_manifest_requires_source_identity_and_unique_targets() {
    let result = ReleaseManifest::from_json(r#"{"schema":1,"version":"0.6.0","bundles":[{"target":"x86_64-unknown-linux-gnu","url":"https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/cli-v0.6.0/bundle.tar.gz","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}"#);
    assert!(result.is_err(), "source commit must bind accepted release identity");
    let result = ReleaseManifest::from_json(r#"{"schema":1,"version":"0.6.0","commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bundles":[{"target":"x86_64-unknown-linux-gnu","url":"https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/cli-v0.6.0/bundle.tar.gz","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"target":"x86_64-unknown-linux-gnu","url":"https://github.com/Cyber-Nomad-Collective/beskid_compiler/releases/download/cli-v0.6.0/other.tar.gz","sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]}"#);
    assert!(result.is_err(), "ambiguous target must fail closed");
}
