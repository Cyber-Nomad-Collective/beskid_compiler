# CLI surface subgate

Run this against the installed CLI artifact on the Linux release builder after
its exact ABI-v5 runtime kit is available. The runner creates temporary
projects, templates, and a loopback registry mock. It never contacts the
production registry or modifies an existing user project.

```sh
python3 -m unittest discover -s scripts/ci -p 'test_cli_surface_gate.py'
python3 scripts/ci/cli_surface_gate.py /absolute/path/to/bin/beskid \
  --expected-sha256 <build-recorded-sha256> \
  --corelib-root /absolute/path/to/release-bundle/beskid_corelib \
  --expected-corelib-fingerprint <independently-verified-fingerprint> \
  --json /absolute/path/to/cli-surface.json
```

`just cli-surface-gate <binary> <sha256> <corelib-root> <fingerprint> <json>` runs both commands where
`just` is installed. The release builder can invoke the Python commands
directly.

Set `BESKID_RUNTIME_PREFIX` to the matching installed runtime kit before
invocation. The Corelib root must be the marked `beskid_corelib` directory in
the installed release archive, with its fingerprint independently checked
against the pinned source. The root release receipt must bind the same
fingerprint to the exact archive and source commits. The gate recomputes it,
checks the marker, copies the bundle into a private fixture, and verifies both
copies remain unchanged after smoke. It never allows the CLI to silently
provision an empty Corelib root. The subgate fails when the binary checksum differs, an advertised
safe leaf is uncovered, a smoke exit/output contract fails, ordinary pipe or
plain output contains control bytes, or the PTY graph/ordinary command
scenarios fail. JSON records the verified Corelib fingerprint; rows include exact stdout and stderr bytes as base64,
decoded text, expected and actual exits, per-path setup skips, and PTY
transcripts. The `graph --tui` case accepts its terminal box renderer whether
or not it uses the alternate screen; the ordinary `analyze --plain` PTY case
rejects terminal control bytes.

The JSON deliberately records `source_provenance.status=unverified` and
`release_qualified=false`. A passing CLI surface run proves behavior of the
checksum-matched binary only. Publication still requires an independent
source-to-binary build receipt, native platform gates, and the other release
evidence. The root release runner must invoke this subgate and attach that
receipt; merely having this script in the repository does not qualify a
release.
