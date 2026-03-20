# Extern examples

This directory contains small extern-focused examples. Build and run via the engine test harnesses or your own driver.

- 01-resolution-only.beskid: declares an extern contract and a dummy main
- 02-getpid.beskid: demonstrates a language-level call `C.getpid()` and returns the PID

Notes:
- On Linux, dynamic linking is gated by `--features extern_dlopen`
- Security: you can restrict externs via BESKID_EXTERN_ALLOW / BESKID_EXTERN_DENY

