# Licensing

The Beskid compiler, command-line tools, language server, reusable Rust crates,
canonical runtime, and compiler-owned templates are licensed under the Apache
License 2.0. The complete terms are in [LICENSE](LICENSE).

The runnable package-registry service in `crates/beskid_pckg_server` is the one
exception: that package is licensed under AGPL-3.0-only. Its complete terms and
scope note live in that directory. Reusable package client, contract, artifact,
authentication, operation, and storage crates remain Apache-2.0.

The `corelib` directory is an independent `beskid_standard` Git submodule and
is licensed under Apache-2.0 by its own repository. Vendored and path-based
third-party dependencies retain their own licenses; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and the license files shipped
with those sources.

## Compiled programs

Using the compiler does not impose the compiler's license on user-authored
source code or generated code. Linked Beskid programs do contain portions of
the Apache-2.0-licensed canonical runtime and may contain compiled portions of
the Apache-2.0-licensed core library. Distributors must preserve the applicable
Apache-2.0 license and attribution notices with those portions. Because these
components are permissively licensed, no compiler-runtime exception is needed.
