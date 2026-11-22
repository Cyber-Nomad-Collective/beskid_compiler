# Beskid browser target

The browser target uses the ordinary project resolver, semantic analysis and ISLE lowering. It emits a WebAssembly module with the configured entrypoint (`Main` by default) and the application’s explicitly named exports. Use an app project and the same installed Corelib that you use for other Beskid builds. [CLI target selection](../beskid_cli/src/commands/build.rs)

```sh
beskid build --wasm --project App.bproj --output app.wasm
```

The target reuses the compiler’s 64-bit native-profile CLIF and checks it before passing it to `clif2wasm 0.1.0`. It maps supported managed layouts to engine-collected Wasm GC `struct` and `array` objects. A managed invocation owns its root storage. The host preserves the caller’s roots on re-entry and the Wasm suspension wrapper retains the current roots across JSPI. Modules with GC operations or loop polls use the checksummed Binaryen tools in the installed kit. The `--release` flag also uses `wasm-opt`. [Emission and linking](src/lib.rs) · [Managed lowering](src/managed_clif.rs)

Debug builds emit a relative, content-addressed source map alongside the module. Deploy the referenced `beskid-*.wasm.map` file to let a debugger resolve Wasm program counters to Beskid function declarations. The map embeds the compiled sources. In a release build, pass `--source-map` to include it. After inlining, the map identifies the surviving caller.

Inline CLIF can load and store bytes in typed array payloads, including unaligned and explicitly ordered fields. The browser backend maps those operations to Wasm GC array accesses. Crypto keeps its shared Beskid hash and cipher kernels; its optional OpenSSL provider reports unavailable in the standard browser host. Modules using SIMD declare it in `beskid.browser.requiredFeatures`.

## Browser ABI

Modules carry a `beskid.browser` custom section with `formatVersion: 2`, `hostInterfaceVersion: 2`, required features, export types and reachable host service signatures. Exported functions accept scalars, strings, copied `u8[]` buffers and typed managed references, including Beskid closures. A closure can be passed back to an export with the same parameter and result types. A managed result produces an opaque object owned by its instance. Pass it back to a matching export to preserve identity and mutation. Call `release()` to sever its reference early. The host rejects released values, foreign instances and mismatched types. JavaScript uses booleans for `bool`, Unicode scalar numbers for `char`, and `BigInt` for `i64`, `u64` and `word`. Strings cross by value as UTF-8 copies. Serve the compiler-emitted `beskid.js` alongside the module. [Host source](browser.mjs)

```js
import { instantiateBrowser } from "./beskid.js";

const module = await WebAssembly.compileStreaming(fetch("./app.wasm"));
const app = instantiateBrowser(module);
try {
  console.log(await app.exports.Main());
} finally {
  app.dispose();
}
```

Bindings are imported only when used. The current standard services have version 1 within host interface version 2.

| Import from `env` | Return value |
| --- | --- |
| `clock_monotonic_nanos() -> i64` | Monotonic time in nanoseconds from the browser time origin |
| `clock_realtime_nanos() -> i64` | Unix time in nanoseconds, at the resolution of `Date.now()` |
| `fiber_now_millis() -> i64` | Monotonic time in milliseconds |
| `beskid_rt_v5_fiber_yield() -> ()` | Suspends through JavaScript Promise Integration (JSPI) and resumes after a queued browser task |
| `beskid_rt_v5_external_sleep_until(i64 deadline) -> i64` | Suspends through JSPI until a monotonic nanosecond deadline, then returns the runtime status. The public `Time.Sleep` maps the result through its existing Corelib variants. |
| `syscall_write(i64 fd, i64 text) -> i64` | Writes UTF-8 text to the browser console. Descriptors `1` and `2` are stdout and stderr, with the result measured in bytes. A console adapter receives the decoded string. |
| `str_from_f64_bits(i64 bits) -> i64` | Formats an IEEE-754 value using the ECMAScript decimal layout and returns a managed UTF-8 string handle. Chromium verifies signed zero, finite values, decimal exponents, infinity and NaN. |
| `log`, `log2`, `log10`, `sin`, `cos`, `tan` | JavaScript `Math` implementations with one `f64` argument and one `f64` result |
| `atan2(f64 y, f64 x) -> f64` | JavaScript `Math.atan2` |

The numeric bindings accept the default C library and explicit non-optional C declarations for library `m`. The compiler checks arity and the full CLIF signature. A different library or signature produces a build error. [Numeric binding checks](src/math.rs)

The `clock`, `math`, `scheduler`, `console`, `files`, `arguments`, `environment`, `http` and `websocket` services support explicit per-instance overrides through `instantiateBrowser(module, { adapters })`. Each entry is keyed by service ID and supplies `service`, `version: 1`, and a `methods` object covering the imported bindings for that service. An optional `dispose()` closes resources owned by the adapter. The facade exposes frozen capability entries with the version and provider. A bad adapter descriptor fails before instantiation.

For modules with a suspending import, the host requires `WebAssembly.Suspending` and calls the export through `WebAssembly.promising`. A suspending adapter method receives an `AbortSignal` as its last argument. Disposal aborts pending host work and closes the instance facade. [Host source](browser.mjs) · [Suspension checks](src/suspension.rs)

### Application composition

`launch` and `with` use the source-resolved composition plan on both targets. Each browser invocation keeps its own registrations and scope stack. A suspended invocation retains its injected values. Disposal hooks run once when the invocation leaves a completed activation, including when a trap escapes to JavaScript.

`CommandLine.Exit(code)` ends the current invocation and cancels its pending fibers and host operations. Its scoped disposal hooks run innermost first, followed by global disposal. An exported function returning `i32` completes with that status. Callers of an export with another result type receive a `BeskidProgramExit` error with a numeric `code` property. Other invocations and browser instances continue independently.

## Corelib coverage

The checks use ordinary app manifests with the checkout’s shared Corelib packages. Resolving the aggregate makes package sources available to the compiler. Each check exercises the functions reachable from its entrypoint. [Chromium verifier](../../../examples/wasm-reuse-poc/verify-cli.sh)

| Package or feature | Verified behavior |
| --- | --- |
| Foundation, `Core.Math` | `Math.Clamp(Math.Abs(-51), -7, 42)` executes in Chromium and returns `42`. The module has zero imports. |
| Foundation, floating-point math | The ordinary Corelib project exercises floor, ceiling, square root, logarithms and trigonometry in Chromium. Disassembly shows `f64.floor`, `f64.ceil` and `f64.sqrt` for the first three. Only the seven transcendental operations use host imports. |
| Concurrency, `Concurrency` | Two `Concurrency.NowMillis()` calls pass a monotonicity check. The same Chromium program reads the positive `nanos` field of the `Instant` returned by `Time.NowUtc()`. |
| Concurrency, `Yield` | A queued browser task runs while the Wasm invocation is suspended. Two nested calls resume and return `42`. |
| Foundation strings | UTF-8 literals, concatenation, equality, ordinal comparisons, byte length, slicing, byte indexing and the shared `Utf8.FromUtf8CodeUnits` facade run in Chromium. Integer interpolation includes the minimum signed 64-bit value. String data and these operations stay in Wasm GC. |
| Foundation, `Core.Bytes.Slice` | `New` and `Len` run through the shared typed array plans. Chromium tests verify zero-length buffers, dynamic allocation, zero initialization and mutable indexing. |
| Cooperative synchronization | Fibers, channels, mutexes, hubs and wait groups use the instance scheduler. A channel keeps queued Wasm GC values alive until a receiver claims them. The entrypoint waits for owned children and detached fibers continue on the event loop. |
| Isolate workers and ports | Reachable operations requiring native isolated workers and cross-heap port transit fail with a browser-target diagnostic. The browser exposes its own main isolate identity to Corelib. |
| Wasm GC | One function constructs a record containing an array and another sums it in Chromium, returning `22`. A single record can hold both `i32[]` and `f64[]`. In the allocation-loop test, 1,200 consecutive allocations use one handle slot. |
| Network, `TcpListener.Bind` | A valid loopback bind returns `NetworkError::Unsupported` through the shared Corelib code in Chromium. |
| Foundation, `Core.FS` | Read, write and delete UTF-8 text below `opfs:/` in origin-private storage. The host suspends through JSPI. |
| Foundation, arguments and environment | An instance starts with empty virtual arguments and environment. Environment mutation is instance-local. |
| Network, `Dns.Resolve` | The standard browser host returns the shared `NetworkError::Unsupported` for a valid direct DNS query. |
| Network, explicit adapter | Chromium exercises TCP bind, address lookup, accept, stream read and write, close, and DNS resolution in two isolated instances. It checks permission denial, oversized adapter output and cancellation at disposal. |
| Web, `WebClient.Send` | Two concurrent POST requests use Fetch in Chromium. The server receives the header and byte body and responds with HTTP 202 and the same byte. An unsupported URL scheme returns the Corelib error. |
| WebSocket, `WsConnection` | Two concurrent Chromium clients open connections, send binary data, receive echoes and close through the shared facade. An unsupported URL scheme returns the Corelib error. |
| Crypto, `Entropy.Fill` and `Bytes` | The shared corelib validates lengths and wipes the buffer on failure. The browser calls Web Crypto in chunks of up to 65,536 bytes. |
| Concurrency, `spawn` | The compiler-emitted trampoline runs a child with scalar or managed arguments. Join returns the typed result after suspension. Chromium tests cover nested fibers, cancellation of a sleeping child, independent siblings, detached execution and a trapped child returning `FiberError::Panicked`. |
| Language syntax, `async` and `await` | The parser rejects both keywords before code generation. Beskid reserves them and uses `spawn` and fiber operations for native concurrency. |

The TCP probe calls `TcpListener.Bind` with a loopback address, port zero and backlog one. The browser lowering supplies the portable unsupported status to the same error handler used by the native target. [Public bind API](../../corelib/packages/network/src/Network/Tcp/TcpListener.bd) · [Error handling and service call](../../corelib/packages/network/src/Network/Internal.bd)

Browser clock readings are synchronous. A pending yield or timer can be canceled at instance disposal. The default file host accepts explicit paths such as `opfs:/notes.txt`. An application can inject a version-1 `files` adapter to work with handles granted by the user. Text reads return `{ status, text }`; mutations and existence checks return the Corelib filesystem status. Async adapter methods receive an AbortSignal. [Browser import allowlist](src/lib.rs) · [Managed lowering](src/managed_clif.rs) · [Reserved syntax](../beskid_analysis/src/parsing/reserved_keywords.rs)

For browser builds, the compiler selects the client implementations shipped with the matching Corelib package sources. Fetch controls redirects, credential policy, CORS, certificate validation and negotiation. The response contains the status, reason, headers and bytes exposed by Fetch. `WebResponse.protocol` is empty because Fetch does not expose the negotiated protocol. The WebSocket implementation supports complete text and binary messages and browser-compatible close codes. It reports the browser's origin and negotiated subprotocol. Raw frames, pings, listener accept and custom TLS or origin settings are unavailable through the standard browser APIs. Disposal aborts pending operations and closes connections owned by that instance. The release gate runs both clients against a local server in Chromium. [Protocol implementation](browser_io.mjs) · [Chromium protocol test](../../../examples/wasm-reuse-poc/corelib-protocols/verify.sh)

### Explicit network adapter

An application can install a per-instance `network` adapter with `service: "network"`, `version: 1` and any subset of the methods below. Missing operations return the existing Corelib unsupported result. The standard host uses that outcome for raw TCP, UDP and DNS. Provider methods operate on opaque JavaScript socket objects and copied byte arrays. The host assigns their Beskid handles, checks returned data, and closes all remaining sockets before calling the adapter's `dispose()`.

Provider method names have the prefix `beskid_rt_v5_network_`. Status values are numbers from the portable Corelib network status table. Status zero signals success. Counts, deadlines, option masks, ports and family values passed to the adapter are `BigInt`. Addresses use the 19-byte Corelib wire format. A deadline of `-1n` leaves the wait unbounded. Operations with an `AbortSignal` may return a promise. [Network status and address format](../../corelib/packages/network/src/Network/Internal.bd) · [Networking adapter test](../../../examples/wasm-reuse-poc/tests/network-adapter.mjs)

| Method suffix and arguments | Result |
| --- | --- |
| `open({ kind, address, options, backlog, deadline }, signal)`, `accept(resource, deadline, signal)` | `{ status, resource }`. Open kinds 1, 2 and 3 mean TCP listener, TCP stream and UDP socket. A provider that creates sockets also supplies `close`. |
| `close(resource)`, `shutdown_write(resource)`, `set_options(resource, options)`, `set_deadlines(resource, read, write)` | Status |
| `address(resource, peer)` | `{ status, address }` |
| `options(resource)` | `{ status, options }` |
| `read(resource, limit, deadline, signal)` | `{ status, bytes }` |
| `write(resource, bytes, signal)`, `send(resource, bytes, address, deadline, signal)` | `{ status, count }`. Connected datagram sends receive a null address. |
| `receive(resource, limit, connected, deadline, signal)` | `{ status, bytes, address, truncated }` |
| `udp_connect(resource, address, signal)` | Status |
| `dns_resolve(host, port, family, deadline, signal)` | `{ status, addresses }`. The host owns the resulting address list. |

The browser checks ran with Chromium 138, Node.js 24.21.0 and Binaryen 132. The managed cases require a Wasm GC engine.

For a compiler built from the checkout, stage the release-matched Binaryen kit once. From `compiler/` on x86-64 Linux, run `node scripts/stage-wasm-kit.mjs x86_64-unknown-linux-gnu target` and set `BESKID_RUNTIME_PREFIX` to the absolute path of `target`. Pass a verified local Binaryen archive as the optional final argument to stage offline. Release bundles include the tools. The compiler verifies them against its embedded checksums and does not search PATH.
