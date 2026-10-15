/* Completion-port settlement rules. Pure, platform-independent decisions used by
 * the IOCP transport (x86_64-pc-windows-msvc/network_iocp.h) and tested on every host
 * (tests/fixtures/network_settle.c). No Windows header is required: the Win32
 * completion codes the rules depend on are fixed numeric values of the Windows SDK,
 * and network_iocp.h asserts their parity at compile time.
 *
 * Model (docs/design/2026-10-12-iocp-deadline-race.md):
 * - An overlapped operation that was accepted by the kernel always queues exactly one
 *   completion packet, also after CancelIoEx. CancelIoEx is a request, not a result.
 * - The final status of the operation exists only in that dequeued packet.
 * - A runtime cancel (deadline or fiber cancellation winner) therefore SETTLES the
 *   operation: it issues CancelIoEx and waits, for at most BESKID_SETTLE_WAIT_MS, until
 *   the reactor dequeues the packet. The packet result is written to the request, so a
 *   transfer that completed before the cancel took effect wins over the deadline, as on
 *   the readiness backends.
 * - When the bound expires first, the operation is ABANDONED: the request is detached
 *   with a cancelled status, and the operation (OVERLAPPED, native buffer, accept
 *   socket) stays in its socket slot, owned by the reactor, until its packet arrives.
 *   The slot rejects new operations of that direction as busy meanwhile. A late packet
 *   that moved stream bytes poisons the socket, because the caller was told that
 *   nothing moved.
 * - A native close ORPHANS the operation: the close winner is already decided, and the
 *   packet only releases the OVERLAPPED storage. Closing the handle forces the packet. */
#ifndef BESKID_NATIVE_NETWORK_SETTLE_H
#define BESKID_NATIVE_NETWORK_SETTLE_H
#include <stddef.h>
#include <stdint.h>

#define BESKID_SETTLE_ERROR_NETNAME_DELETED 64u
#define BESKID_SETTLE_ERROR_MORE_DATA 234u
#define BESKID_SETTLE_ERROR_OPERATION_ABORTED 995u
#define BESKID_SETTLE_ERROR_NOT_FOUND 1168u
#define BESKID_SETTLE_WSAEMSGSIZE 10040u

/* Upper bound of one settling cancel. Winsock (AFD) completes a cancelled receive,
 * send, AcceptEx, or ConnectEx at once, so the bound is reached only when the reactor
 * thread is starved or a layered provider withholds the packet. The settling cancel
 * holds the runtime network table lock, so the bound also limits how long other
 * network calls of the process wait behind it. */
#define BESKID_SETTLE_WAIT_MS 250u
/* Upper bound of the reactor drain at shutdown, after every socket handle is closed
 * (which forces the packet of each remaining operation). Operations still pending
 * after it are leaked by design together with the reactor (see network_iocp.h). */
#define BESKID_SETTLE_DRAIN_MS 2000u

/* Mirrors NET_CONNECT..NET_SEND in network.h; network_iocp.h asserts parity. */
enum { BESKID_SETTLE_CONNECT = 1, BESKID_SETTLE_ACCEPT, BESKID_SETTLE_READ, BESKID_SETTLE_WRITE,
  BESKID_SETTLE_RECEIVE, BESKID_SETTLE_SEND };

/* Lifecycle of one overlapped operation with respect to its runtime request. */
enum BeskidSettleState {
  BESKID_SETTLE_IN_FLIGHT = 0, /* request attached; packet result is delivered and posted */
  BESKID_SETTLE_SETTLING,      /* request attached; a cancel waits for the packet */
  BESKID_SETTLE_ORPHANED,      /* request detached, socket closed; packet only frees native storage */
  BESKID_SETTLE_ABANDONED      /* request detached after the settle bound; operation keeps its
                                  socket slot until the packet; packet decides stream sync */
};

/* What one dequeued packet means for the operation. */
enum BeskidSettleOutcomeKind {
  BESKID_SETTLE_TRANSFERRED = 0, /* the operation completed; count bytes moved */
  BESKID_SETTLE_TRUNCATED,       /* a datagram was consumed and truncated to capacity */
  BESKID_SETTLE_ABORTED,         /* CancelIoEx took effect before any transfer */
  BESKID_SETTLE_FAILED           /* a transport error; map error with the native table */
};
struct BeskidSettleOutcome { int kind; size_t count; };

/* Classify a dequeued packet: error is 0 when GetQueuedCompletionStatus succeeded,
 * else its GetLastError() value; bytes is the transferred byte count of the packet.
 *
 * A stream read or write that reports ERROR_OPERATION_ABORTED with a nonzero byte
 * count moved those bytes before the cancel took effect. Discarding them would drop
 * received stream data or misreport sent data, so they count as a completed transfer.
 * Datagram, accept, and connect operations are atomic; an aborted one did not transfer. */
static inline struct BeskidSettleOutcome BeskidSettleClassify(uintptr_t operation, uint32_t error, size_t bytes, size_t capacity) {
  struct BeskidSettleOutcome outcome = {BESKID_SETTLE_FAILED, 0};
  size_t moved = bytes > capacity ? capacity : bytes;
  if (error == 0) {
    outcome.kind = BESKID_SETTLE_TRANSFERRED; outcome.count = moved;
  } else if (operation == BESKID_SETTLE_RECEIVE && (error == BESKID_SETTLE_WSAEMSGSIZE || error == BESKID_SETTLE_ERROR_MORE_DATA)) {
    outcome.kind = BESKID_SETTLE_TRUNCATED; outcome.count = capacity;
  } else if (error == BESKID_SETTLE_ERROR_OPERATION_ABORTED) {
    if (moved && (operation == BESKID_SETTLE_READ || operation == BESKID_SETTLE_WRITE)) {
      outcome.kind = BESKID_SETTLE_TRANSFERRED; outcome.count = moved;
    } else outcome.kind = BESKID_SETTLE_ABORTED;
  }
  return outcome;
}

/* A transferred or truncated outcome is a completed operation (runtime status OK). */
static inline int BeskidSettleCompleted(struct BeskidSettleOutcome outcome) {
  return outcome.kind == BESKID_SETTLE_TRANSFERRED || outcome.kind == BESKID_SETTLE_TRUNCATED;
}

/* Reactor action for a dequeued packet, by operation state. */
enum BeskidSettleDelivery {
  BESKID_SETTLE_DELIVER_AND_POST = 0, /* write the outcome to the request; post READY to its owner */
  BESKID_SETTLE_DELIVER_AND_WAKE,     /* write the outcome to the request; wake the settling cancel */
  BESKID_SETTLE_DISCARD               /* free native storage only */
};
static inline int BeskidSettleDelivery(int state) {
  if (state == BESKID_SETTLE_IN_FLIGHT) return BESKID_SETTLE_DELIVER_AND_POST;
  if (state == BESKID_SETTLE_SETTLING) return BESKID_SETTLE_DELIVER_AND_WAKE;
  return BESKID_SETTLE_DISCARD;
}

/* Runtime cancel on an attached operation settles first, because only the packet holds
 * the final status. Native close orphans an in-flight or abandoned operation. A settling
 * operation stays settling on close: its cancel still waits for the packet, which the
 * closed handle forces. The settle bound turns a settling operation into an abandoned one. */
static inline int BeskidSettleOnCancel(int state) {
  return state == BESKID_SETTLE_IN_FLIGHT ? BESKID_SETTLE_SETTLING : state;
}
static inline int BeskidSettleOnClose(int state) {
  return state == BESKID_SETTLE_IN_FLIGHT || state == BESKID_SETTLE_ABANDONED ? BESKID_SETTLE_ORPHANED : state;
}
static inline int BeskidSettleOnExpire(int state) {
  return state == BESKID_SETTLE_SETTLING ? BESKID_SETTLE_ABANDONED : state;
}

/* Bounded settle wait. The transport supplies the clock (milliseconds), the settled test,
 * and a wait that releases its lock for at most the given time; spurious and unrelated
 * wakes are allowed. Returns 1 when the packet was delivered, 0 when the bound expired
 * first. The caller holds its lock on entry and on return, so the result and the
 * operation state cannot change between the last test and the state transition. */
struct BeskidSettleWait {
  void *context;
  uint64_t (*now)(void *context);
  int (*settled)(void *context);
  void (*wait)(void *context, uint32_t milliseconds);
};
static inline int BeskidSettleAwait(const struct BeskidSettleWait *wait, uint32_t bound) {
  uint64_t start = wait->now(wait->context);
  for (;;) {
    if (wait->settled(wait->context)) return 1;
    uint64_t elapsed = wait->now(wait->context) - start;
    if (elapsed >= bound) return 0;
    wait->wait(wait->context, (uint32_t)(bound - elapsed));
  }
}

/* A late packet of an abandoned operation. The caller already received a cancelled
 * status, so any transfer it reports was not delivered. For a stream that breaks the
 * byte order: received bytes are gone from the socket, sent bytes reached the peer, and
 * a late connect left a connection the caller treats as timed out. The socket is then
 * poisoned and fails every later operation. End of stream (a zero-byte read) and an
 * abort leave the stream intact. Datagram loss is within UDP semantics, and a late
 * accepted connection is closed with the operation, which the peer sees as a reset. */
static inline int BeskidSettleAbandonedPoisons(uintptr_t operation, struct BeskidSettleOutcome outcome) {
  if (!BeskidSettleCompleted(outcome)) return 0;
  if (operation == BESKID_SETTLE_CONNECT) return 1;
  return (operation == BESKID_SETTLE_READ || operation == BESKID_SETTLE_WRITE) && outcome.count > 0;
}

/* CancelIoEx result handling: success, or ERROR_NOT_FOUND (the operation already
 * completed and its packet is queued), both guarantee exactly one packet. Any other
 * failure means the handle or OVERLAPPED is not the one the kernel holds. */
static inline int BeskidSettleCancelAccepted(int cancelled, uint32_t error) {
  return cancelled || error == BESKID_SETTLE_ERROR_NOT_FOUND;
}

/* Deadline arbitration after settlement, mirroring Runtime.Network.Operations.NetworkFinish:
 * a completed operation converts a timeout winner into READY. Cancellation and close
 * winners are unchanged. Returns 1 when the completed transfer is delivered. */
static inline int BeskidSettleTransferWinsDeadline(int timeoutWon, struct BeskidSettleOutcome outcome) {
  return timeoutWon && BeskidSettleCompleted(outcome);
}
#endif
