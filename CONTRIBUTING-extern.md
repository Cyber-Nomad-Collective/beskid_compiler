# Extern tests and development

Linux-only dynamic extern tests are gated by the `extern_dlopen` feature.

- Run all engine tests with externs:
  cargo test -p beskid_engine --features extern_dlopen

- Run a focused extern test:
  cargo test -p beskid_engine extern_real_call_getpid --features extern_dlopen

Security controls (opt-in):
- BESKID_EXTERN_ALLOW: comma-separated patterns (e.g., libc.so.6:getpid)
- BESKID_EXTERN_DENY: comma-separated patterns

See also: docs/extern-cookbook.md, docs/extern-policy-v0-1.md

