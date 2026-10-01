# Template-authoring project lock regression

The 0.5.1 CLI interaction specification and the canonical OpenSpec
`tooling--project-scaffolding--project-templates` require generated project
locks to identify the final manifest and contain the verified Corelib closure.
Template authoring roots still cannot compile (E1877); their payload is
instantiated before compilation.

Ruling: replace the old graph expectation that Template roots exclude implicit
Std. The approved generated-project lock requirement now applies to the
first-party `project` template, whose output is another Template package.
Dependency materialization must remain separate from compile-target selection.
This deliberately adds Corelib lock entries to authoring roots; retaining the
old exclusion would violate the approved release gate. Nested Template
dependencies retain their existing graph policy.

The first-party authoring shape has a `content/` payload and no `Src/` compile
tree. A target-free dependency plan must not fabricate a target or require that
missing compile source directory. Compilation paths must retain E1877.

The focused regression command is:

```sh
CARGO_TARGET_DIR=/target/v051-template-author-lock \
BESKID_CORELIB_SOURCE=/workspace/compiler-v051-template-author-lock/corelib \
cargo test -j 2 -p beskid_cli --test template_authoring_lock
```

Run from `/workspace/compiler-v051-template-author-lock` in
`beskid-codex-build` on the NixOS builder. Each CLI fixture supplies a disposable
`BESKID_CORELIB_ROOT`, so it cannot race another slice's installed Corelib tree.
The baseline is compiler `44a07aed`; the first-party template source baseline
for the original quality failure is templates `657dec5`.

The fixture uses the first-party authoring manifest/content shape with a literal
non-default manifest filename. This isolates the E2099 lock post-action failure
from the separate generated-path substitution regression. The root integration
candidate's source-template quality check also exercises the literal first-party
template tree.

RED evidence (2026-10-01): the CLI regression failed in 0.12 seconds with the
Template-root compilation prohibition followed by
`E2099: beskid lock failed with status exit status: 1`. The explicit `--target`
rejection check passed. Log: `/target/v051-template-author-lock-red-final.log`.
The graph regression separately failed in 0.08 seconds because the Template
root omitted implicit Std. Log: `/target/v051-template-author-lock-graph-red.log`.

The implementation derives `ProjectWorkspacePlan` from the same graph used by
compilation. It has no target and an absent compile source root for Template
packages. Only the dependency-operation service admits that authoring shape;
ordinary compilation still reaches the E1877 gate. Existing lock validation,
portable path containment, verified Corelib classification, registry pins, and
locked/frozen policies are shared with compilable projects.

Initial GREEN: `cargo test -j 2 -p beskid_cli --test template_authoring_lock
--test portable_lock_policy --no-fail-fast` passed both authoring regressions
and all 15 existing portable-lock CLI checks. The authoring check covers the
ten-entry verified Corelib closure, unchanged bytes after `update`, relocated
`fetch --locked` and `fetch --frozen`, compilation rejection, absent materialized
compile root, and foreign-owner rejection without lock mutation.
Log: `/target/v051-template-author-lock-green.log`.

The final Std condition applies only to Template graph roots. Nested Template
path dependencies keep their prior policy and remain outside compilation
projection. The full seven-template quality gate belongs to the root integration
candidate, which also includes the independently reviewed path-substitution and
overwrite fixes; this isolated slice does not incorporate those commits.

Final suite verification:

```sh
CARGO_TARGET_DIR=/target/v051-template-author-lock \
BESKID_CORELIB_SOURCE=/workspace/compiler-v051-template-author-lock/corelib \
BESKID_CORELIB_ROOT=/workspace/.corelib-v051-template-author-lock-suite \
cargo test -j 2 -p beskid_cli -p beskid_analysis -p beskid_tests_projects \
  --no-fail-fast -- --test-threads=2
```

Passed: 718 tests across 12 completed test binaries; zero failures. The project
suite reported 98 existing ignored tests. Both doc-test
targets also passed. This run includes the updated Template graph regression,
the unchanged E1877 compile-plan rejection, and nested Template dependency
projection checks. Log: `/target/v051-template-author-lock-suites.log`.

After that full run, the Template graph test gained a direct service-level
assertion that authoring dependency resolution returns no `CompilePlan` and
does not materialize `obj/beskid/root`. The final focused run
`cargo test -j 2 -p beskid_tests_projects projects::templates -- --test-threads=2`
passed all eight Template tests, zero failures, using the same isolated Corelib
settings. Production code did not change between the full and focused runs.
Log: `/target/v051-template-author-lock-final-template-group.log`.

The isolated installed Corelib bundle was provisioned by `beskid new list`
before test execution. Its verified `.beskid-bundle.sha256` fingerprint is
`94e51e87ddc064d42c6b550bb956c5fb569e2dbc6de939747d60f7165dd43cab`.
The source workspace was staged from the existing v0.5.1 library-slice Corelib
snapshot into this slice's own `corelib/` directory; no shared installed-root
or runtime-kit changes were required by these dependency checks.

Strict lint passed with `cargo clippy -j 2 -p beskid_analysis -p beskid_tools
-p beskid_cli --all-targets -- -D warnings`.
Log: `/target/v051-template-author-lock-clippy.log`.
