# Concurrency implementation status

Scope document for the fiber concurrency plan (M0–M8). Normative spec:
`site/website/src/content/docs/platform-spec/core-library/concurrency/concurrency-package/`.

## Milestones

| Milestone | Track | Status | Notes |
| --- | --- | --- | --- |
| **M0** | ABI + builtins | Landed | `beskid_abi::BUILTIN_SPECS`, `beskid_analysis::builtins`, JIT/AOT import of split status/value symbols |
| **M1** | Runtime channels | Landed | `channel.rs`, `channel_receive_status` / `channel_receive_value`, bounded/unbounded FIFO |
| **M2** | Scheduler | Landed (Phase A) | Split `scheduler/{state,tls,spawn,run_loop}.rs`, corosensei stacks, single mutator; park/wake on channels |
| **M3** | `spawn` + analysis | Landed (Phase A) | Spawn lowering calls `fiber_spawn_with_cancel_slot`, returns the `i64` fiber id through the ABI table, and has JIT/corelib coverage |
| **M4** | Corelib package | Landed | `compiler/corelib/packages/concurrency/` thin wrappers over split `__fiber_*`, `__channel_*`, `__hub_*`, `__mutex_*`, `__wait_group_*` |
| **M5** | Sync + cancel | Landed | Mutex, WaitGroup, Hub round-robin; cancel wakes parked channel waiters and raises `Fiber.OnCancelled` |
| **M6** | Syscall parking | Landed (Phase A) | `syscall_pool` parks the current fiber while blocking read/write work runs on worker threads |
| **M7** | Console channel | Landed | `Channel<ConsoleMessage>`, `Console.MessagesChannel()`, `Terminal.PollResize` → `Send(Resize)` |
| **M8** | OS threading | Landed (v1) | `System.Threading.Thread` pthread extern surface; distinct from cooperative `Concurrency.Yield` |
| **M9 (Phase B GC)** | Multi-mutator GC | Landed (opt-in) | Multiple Beskid mutators attach to one shared heap via `attach_phase_b_mutator`; insertion write barrier active on pointer-payload channels; syscall pool workers explicitly tagged and blocked from allocating outside a runtime scope; optional preemption hook (`runtime_preempt_check`) |

## Stable surfaces (v1 Phase A)

- **Channel**: `Create` / `CreateWithOptions`, `Send` → `Result<SendOk, ChannelError>`, `Receive` / `TryReceive`, `Close`
- **Fiber handle**: `Join` / `Detach` / `Cancel`, `event OnCancelled()` on struct (spawn lowering wires the cancel slot through `fiber_spawn_with_cancel_slot`)
- **Hub**: homogeneous `Hub<T>`, `Register`, `WaitReceive` via `hub_wait_receive_status` + index/value builtins
- **Mutex / WaitGroup**: map to runtime status codes in `Concurrency.Status`
- **Clock / yield**: `Concurrency.Yield`, `NowMillis`, `ProcessorCount` → `__fiber_processor_count`
- **Console**: `ConsoleMessage` enum (`Resize`, `Tick`); cross-fiber UI uses `Channel<ConsoleMessage>` not `event`

## Phase B GC (opt-in)

Phase B is wired and tested but not yet the default mode. Enable globally via
`set_runtime_phase(RuntimePhase::PhaseB)` or by exporting `BESKID_RUNTIME_PHASE_B=1`.
Optional preemption hooks are controlled by `set_preemption_enabled` or
`BESKID_RUNTIME_PREEMPT=1`.

What Phase B turns on:

- **Multiple mutators on one heap.** Foreign OS threads register as Beskid mutators by
  holding a `MutatorAttachGuard` from `attach_phase_b_mutator(heap, ctx)`. While the
  guard is alive, the thread shares the same `Heap`/`GcContext` and may allocate, read,
  and write GC-managed pointers.
- **Real `gc_write_barrier`.** The Dijkstra insertion barrier is applied on every
  pointer-payload channel send and receive (`channel_send_ptr`, `channel_try_send_ptr`,
  `channel_receive_ptr`, `channel_try_receive_ptr`) so handles in transit are kept
  reachable during concurrent marking.
- **Pointer-payload channels.** Sender registers the pointer as an external GC handle
  before pushing the handle id onto the queue; receiver resolves the handle, drops the
  registration, and applies the receiver-side barrier. OS-thread mutators outside the
  fiber scheduler fall back to `try_send`/`try_receive` plus `thread::yield_now`
  instead of `park_current`.
- **Syscall-pool guard.** `syscall_worker` threads call `set_syscall_pool_worker()`;
  `assert_mutator_allowed()` panics if a syscall worker tries to drive `Heap::alloc`
  (or any path that reaches `with_current_root`) without explicitly entering a runtime
  scope (`enter_runtime_scope` / a fresh `MutatorAttachGuard`). This catches accidental
  allocations from threads designed not to be mutators.
- **Optional preemption.** `runtime_preempt_check` is a no-op by default; once
  preemption is enabled it yields the current fiber (or `thread::yield_now` if called
  off the fiber scheduler) so JIT/AOT codegen can plant safe-point calls at function
  prologues without paying the cost unless preemption is requested.

Stress coverage lives in `crates/beskid_runtime/tests/phase_b_concurrency.rs`
(`phase_b_stress_many_mutators_concurrent_allocations`,
`pointer_channel_round_trip_applies_write_barrier`,
`pointer_channel_cross_thread_with_phase_b_mutators`,
`syscall_pool_worker_without_scope_blocks_alloc`,
`syscall_pool_worker_with_runtime_scope_can_allocate`,
`preemption_check_*`, `phase_b_enables_via_setter`).

## Known gaps

- **`SetProcessorCount`**: corelib no-op until runtime exposes dynamic worker resize (no `__fiber_set_processor_count`)
- **`Result<unit, _>`**: `Send` uses zero-size `SendOk` struct until `unit` is a valid `Result` payload type
- **Generic channel payloads at codegen**: ABI exposes `channel_*_ptr` builtins; corelib `Channel<T>` lowering for non-`i64` payloads is still pending
- **Phase B as default**: still opt-in via `set_runtime_phase` / `BESKID_RUNTIME_PHASE_B`; the scheduler boots in Phase A
- **Preemption code emission**: `runtime_preempt_check` is reachable from the ABI; AOT/JIT prologue insertion of the safe-point call is not yet wired

## Safe test commands

Avoid full `nox` on memory-constrained hosts (large `RUST_MIN_STACK` + parallel tests).

```bash
# Runtime unit tests only (no scheduler OOM from corelib lowering)
cd compiler
cargo test -p beskid_runtime concurrency -- --test-threads=1

# Compiler integration (subset; needs workspace compiling)
cargo test -p beskid_tests runtime:: -- --test-threads=1

# Corelib Beskid tests (one target at a time; build CLI first)
cargo build -p beskid_cli -q
cargo run -p beskid_cli --quiet -- test \
  --project corelib/beskid_corelib/tests/corelib_tests \
  --target ConcurrencyChannelApiTests

# Scoped nox (16 MiB stack, single-threaded beskid_tests)
nox -s corelib_quality   # corelib_tests targets + projects::corelib::
nox -s test              # default beskid_tests with --test-threads=1
```

Do **not** run the full compiler `nox -s test` plus all corelib CLI targets in parallel on low-RAM CI without raising limits.

## Debugging memory growth

Use a **single-threaded, bounded** reproduction before attaching profilers. Full-workspace parallel `cargo test` and `nox` sessions with a 64 MiB stack hide scheduler leaks and can OOM the host.

### Reproduce

```bash
cd compiler
# One runtime integration test, no parallel siblings
timeout 120 cargo test -p beskid_runtime concurrency -- --test-threads=1 --nocapture

# Or one beskid_tests runtime case
timeout 120 cargo test -p beskid_tests runtime::sched -- --test-threads=1 --exact <test_name>
```

If RSS climbs while CPU stays hot, suspect a **busy scheduler loop** (`run_main_fiber` spinning with no progress) or **unbounded channel queues** (senders not blocked, receivers gone).

`run_main_fiber` must not allocate on every idle spin: it should only call `thread::yield_now()` when no fiber ran and the run queue is empty. Avoid per-iteration `HashMap` rebuilds (for example join snapshots) on the idle path.

### macOS

| Tool | Use |
| --- | --- |
| **Instruments → Leaks / Allocations** | Time-profile a single `cargo test` binary; filter stacks containing `beskid_runtime::scheduler`, `channel`, `corosensei`. |
| **`heaptrack`** | `heaptrack target/debug/deps/beskid_runtime-<hash>` then `heaptrack_gui` for alloc hotspots. |
| **`cargo instruments`** (Xcode CLI) | `cargo instruments -t Allocations --test concurrency --package beskid_runtime -- --test-threads=1` |

### Linux

| Tool | Use |
| --- | --- |
| **`valgrind --tool=massif`** | Peak heap over a single test: `valgrind --tool=massif target/debug/deps/beskid_runtime-<hash> concurrency --test-threads=1`; view with `ms_print`. |
| **`heaptrack`** | Same workflow as macOS. |
| **`dhat-heap`** | Add `dhat` as a dev-dependency, call `dhat::Profiler::new_heap()` at test start, run one test, inspect `dhat-heap.json`. |

### Rust / nightly

- **`dhat`**: lightweight heap attribution for one test crate without Valgrind slowdown.
- **`mimalloc` + profiling**: link mimalloc, enable its heap profiling build flags when comparing before/after scheduler changes.
- **AddressSanitizer** (nightly tests only): `RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -p beskid_runtime concurrency -- --test-threads=1` — catches UAF/double-free in fiber stacks and channel wait lists; not a leak meter.

### Bisect workflow

1. Run the smallest failing test with `timeout` and `--test-threads=1`.
2. If growth is linear in wall time, log iteration count inside the scheduler loop (debug build) or break on `run_main_fiber` in lldb.
3. Bisect `beskid_runtime/tests/concurrency.rs` and `beskid_tests/src/runtime/` cases until one test remains.
4. Inspect **channel** `VecDeque` lengths and **pending wake/spawn** vectors for drain-without-cap growth.

### What not to run

- Full workspace `cargo test` with default parallelism while debugging scheduler memory.
- `nox -s test` on memory-tight hosts without the repo’s reduced stack / single-thread settings (see [Safe test commands](#safe-test-commands)).
- Long-running corelib CLI test matrices in parallel with runtime stress tests.

## Files touched by track

### Runtime (M0–M2, M5–M6)

- `crates/beskid_abi/src/builtins.rs`, `symbols.rs`
- `crates/beskid_runtime/src/channel.rs`, `hub.rs`, `mutex.rs`, `wait_group.rs`
- `crates/beskid_runtime/src/scheduler/{state,tls,spawn,run_loop,syscall_pool}.rs`
- `crates/beskid_runtime/src/builtins/{channel,fiber,hub,mutex,wait_group}.rs`
- `crates/beskid_runtime/tests/concurrency.rs`
- `crates/beskid_tests/src/runtime/`

### Compiler (M3, M0)

- `crates/beskid_analysis/src/builtins.rs`
- `crates/beskid_codegen/`, `crates/beskid_engine/src/jit_module.rs`
- `crates/beskid_analysis` spawn/HIR (in progress)

### Corelib (M4, M7, M8)

- `corelib/packages/concurrency/**`
- `corelib/packages/console/src/Console.bd`, `Console/ConsoleMessage.bd`, `Platform/Terminal.bd`
- `corelib/Workspace.proj`, `beskid_corelib/Project.proj`
- `corelib/beskid_corelib/tests/corelib_tests/src/concurrency/`, `.../console/ConsoleMessageChannelTests.bd`

### Hygiene

- `compiler/noxfile.py` — `RUST_MIN_STACK` 16 MiB for test sessions; `--test-threads=1` on runtime-related `beskid_tests` invocations
