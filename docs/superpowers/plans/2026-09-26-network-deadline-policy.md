# v0.5 Network deadline policy implementation plan

## Goal and seam

Implement the approved v0.5 `TcpStream.SetDeadlines(TransferDeadlines)` policy without changing the fixed `Core.IO.Stream` method interface. The public module accepts opaque `Option<Deadline>` values; the runtime module owns policy storage, pending-wait rekey, and winner arbitration. No caller or native socket backend owns a second timer.

The parent is `codex/v05-deadline-foundation` (`88d2c9b8`). Its owner-only `ExternalWaitSetDeadline` is already verified on Linux, macOS, and the Windows native fixture. This branch must not be pushed or merged without approval. Do not use GitNexus.

## Behavior to pin

- `SetDeadlines` replaces read and write policies together. New operations observe the new pair, never one old and one new value.
- A pending read or write adopts its direction's replacement deadline. `None` removes its pending timer without finishing the operation.
- An expired replacement may time out the pending operation exactly once. Read and write report `IoError` with `TransferFailure::TimedOut`; the stream remains usable afterward.
- If readiness, cancellation, or close already won, a late rekey does not resurrect the operation. If a rekey command is published before a later readiness command, the owner processes them in that order.
- The public interface never exposes raw monotonic nanoseconds. Only the canonical `Network.Internal` source projects `Deadline`.

## Task 1: deadline-bearing owner command

1. Extend the native owner mailbox command in `crates/beskid_abi/assembly/common/external_wait.h` to carry a signed absolute deadline and a distinct rekey command kind. Keep completion commands unchanged. Use the existing mailbox lock and FIFO. Update its native transport fixture first and observe a red test before implementing.
2. Add an intrinsic for publishing a rekey command. Regenerate the ABI-v5 artifacts from `runtime_manifest.bsol` on the Linux builder. Adjust the Beskid scheduler's command buffer and `ExternalPump` to call `ExternalWaitSetDeadline` for rekey commands, while completion commands still call `ExternalTryComplete`.
3. Test stale-generation, already-won, clear, immediate expiry, and queue-order cases at the owner transport and `external_wait_native` seams. Never let a foreign thread edit owner scheduler state directly.

## Task 2: runtime stream policy

1. Add a failing runtime-native test for a pending read whose deadline is set, advanced, cleared, and finally completed by peer data. Add an expired-pending-read case and a read/write atomic-policy case. Use deterministic owner pump steps where possible; avoid sleeps as assertions.
2. Store both absolute deadline values in the language-owned socket table under `network_table_lock`; initialize each to `-1`. Expand slot/table sizes deliberately and audit every offset. Keep the native reactor's view of the table opaque.
3. Add `beskid_rt_v5_network_set_deadlines(handle, read, write)` as one internal runtime operation. Under the table lock, update both policy values and publish rekey commands for pending direction tokens to their recorded owners before returning. A stale or terminal token is a no-op; a missing live owner is fail-closed, not silent success. Later `NetworkStartLocked` registrations read the stored policy.
4. Keep the existing raw TCP-read deadline argument only as an internal compatibility/input path for current runtime tests. Do not expose it publicly. TCP writes consume the stored write policy without changing `Core.IO.Writer.Write`.

## Task 3: typed corelib interface and authority

1. Add compile-time red tests for `TransferDeadlines { read: Option<Deadline>, write: Option<Deadline> }` and `TcpStream.SetDeadlines`, including rejection of raw integer and `Instant` inputs.
2. Add the public type and method in corelib. The canonical `Network.Internal` module converts the two opaque options to `i64`/`-1` and calls only the new manifest-owned operation. Extend the service registry and compiler authority test so an unregistered or forged call is rejected before JIT/AOT.
3. Add loopback tests for timeout then reuse, pending clear then readiness, and close/cancel/readiness/deadline one-winner races. Assert `IoError` cause and ownership, not just exit status.

## Verification and integration

- Use `scripts/diagnose/` and `docs/diagnose.md` for every new failure; preserve an exact red-to-green log for each task.
- Heavy builds and Linux matrices run in an isolated builder checkout with at most four heavy jobs. Windows uses the key-only VM and a matching native kit; macOS runs only with at least 20 GiB free.
- Run ABI contract and source-authority tests, native JIT/AOT/kit tests, runtime 7/7, and corelib 81/81 on Linux, macOS, and Windows. Record source/kit hashes and distinguish matrix pass count from `release eligible`.
- Commit compiler and corelib changes on their isolated branches. No push or merge to main without the user's approval.
- Separate follow-on slices implement explicit lifecycle deadlines for Connect/Accept/DNS/UDP and any HTTP deadline policy; this stream slice must not claim those requirements complete.
