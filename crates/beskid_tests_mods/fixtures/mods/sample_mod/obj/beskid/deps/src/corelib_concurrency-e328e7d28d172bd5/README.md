# corelib_concurrency

Cooperative concurrency primitives for the Beskid standard library (`Fiber`, channels). OS-thread helpers live in `Core.Threading` (foundation); deprecated `System.Threading` shims remain here for one release.

Depends on `corelib_foundation`. OS-thread helpers live in this package; the aggregate `corelib` prelude re-exports the supported surface.
