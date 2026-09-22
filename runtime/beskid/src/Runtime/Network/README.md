# Canonical runtime networking

`Table.bd` owns the process-wide socket table, packed generation-tagged handle,
admission state, and idempotent close. `Operations.bd` owns request lifetime and
uses the Foundation external wait API. `Sockets.bd` owns public socket service
implementations. `Dns.bd` owns resolver result leases, late-result disposal,
host-work accounting, and network shutdown.

The native layer owns system calls and epoll/kqueue/IOCP mechanics. It posts
immutable `(owner, token, source)` completions through Foundation; it never
changes a fiber state. `network_cancel` must atomically detach a request before
returning, retaining any independent IOCP storage until its completion arrives.
This lets Beskid free the runtime request without a late write into freed memory.

## Runtime/corelib services

All raw services are called only from private `Network.Internal`. Each service
name begins with `__network_`; its runtime export replaces that prefix with
`beskid_rt_v5_network_`. A pointer parameter representing an array points to
the managed array descriptor (`data@0`, `len@8`), not directly to its elements.
The compiler must retain its established rooted array owner across the call.

| Service suffix | Parameters after lowering | Return |
| --- | --- | --- |
| open | i64 kind, pointer address, i64 options, i64 backlog, pointer handleOut | i32 status |
| accept | word handle, pointer handleOut | i32 status |
| close | word handle | i32 status |
| read / write | word handle, pointer bytes, i64 offset, i64 count | i64 count or negative status |
| address | word handle, bool peer, pointer addressOut | i32 status |
| options | word handle | i64 option bits or negative status |
| set_options | word handle, i64 bits | i32 status |
| shutdown_write | word handle | i32 status |
| udp_connect | word handle, pointer address | i32 status |
| receive | word handle, pointer bytes, pointer metadata, bool connected | i64 count or negative status |
| send | word handle, pointer bytes, pointer address, bool connected | i64 count or negative status |
| dns_resolve | pointer stringView, i64 port, i64 family, pointer resultOut | i32 status |
| dns_count | word privateResultLease | i64 count or negative status |
| dns_address | word privateResultLease, i64 index, pointer addressOut | i32 status |
| dns_release | word privateResultLease | unit |

Open kind is listener=1, stream=2, UDP=3. Handle low 32 bits are slot+1 and
high 32 bits are generation; zero is invalid. Generation exhaustion retires a
slot. Native socket identity exists only inside the runtime table.

Portable address wire data has 19 bytes: family 4 or 6, two big-endian port
octets, and 16 address octets. IPv4 uses the first four address octets with a
zero suffix. Receive metadata appends the truncation byte at offset 19.
No wire field carries an OS descriptor, native family value, errno, or WSA code.

Status zero is success. Statuses 1..18 follow the declaration order in
`Network.Errors.NetworkError`; 19 is native pending and cannot reach corelib.
A full socket table, a failed runtime or native allocation, and host
descriptor, buffer, or memory exhaustion report 18 (`ResourceExhausted`), never
9 (`NetworkDown`).
Options bits 0 and 1 represent no-delay and keep-alive, independent of native
socket option constants.

## Foundation integration

`ExternalPump` calls `NetworkPump` before and after arbitration. A resolver
holds a distinct `ExternalWorkRetain` lease until the host job actually exits;
cancelling its ordinary wait does not release that lease. Pump cleanup works
even when a detached fiber is never resumed.

`NetworkShutdown(owner)` runs after the owner's work drains and before fiber
storage and the Foundation owner queue are destroyed. It returns the number of
live socket leaks, emits descriptor-free diagnostics, closes those resources,
and releases requests owned by detached stacks. The caller must fail the leak
check when the result is nonzero. The reactor is destroyed once no live sockets,
requests, or DNS results remain. Generation tombstones persist for the process
lifetime so recreating a scheduler cannot resurrect a stale handle.

The generated request prefix is 88 bytes. Beskid privately allocates 104 bytes:
the additional list-next word at 88 and owner word at 96 permit cleanup after
forced fiber shutdown. Native adapters must access only the generated prefix.

This API uses `ExternalWaitRegister(..., -1)` because there is no Foundation
ambient deadline context. It must not introduce a fabricated deadline getter
or a competing network timer implementation.
