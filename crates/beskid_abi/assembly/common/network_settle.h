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
 *   operation: it issues CancelIoEx and waits until the reactor dequeues the packet.
 *   The packet result is written to the request, so a transfer that completed before
 *   the cancel took effect wins over the deadline, as on the readiness backends.
 * - A native close ORPHANS the operation: the close winner is already decided, and the
 *   packet only releases the OVERLAPPED storage. */
#ifndef BESKID_NATIVE_NETWORK_SETTLE_H
#define BESKID_NATIVE_NETWORK_SETTLE_H
#include <stddef.h>
#include <stdint.h>

#define BESKID_SETTLE_ERROR_NETNAME_DELETED 64u
#define BESKID_SETTLE_ERROR_MORE_DATA 234u
#define BESKID_SETTLE_ERROR_OPERATION_ABORTED 995u
#define BESKID_SETTLE_ERROR_NOT_FOUND 1168u
#define BESKID_SETTLE_WSAEMSGSIZE 10040u

/* Mirrors NET_CONNECT..NET_SEND in network.h; network_iocp.h asserts parity. */
enum { BESKID_SETTLE_CONNECT = 1, BESKID_SETTLE_ACCEPT, BESKID_SETTLE_READ, BESKID_SETTLE_WRITE,
  BESKID_SETTLE_RECEIVE, BESKID_SETTLE_SEND };

/* Lifecycle of one overlapped operation with respect to its runtime request. */
enum BeskidSettleState {
  BESKID_SETTLE_IN_FLIGHT = 0, /* request attached; packet result is delivered and posted */
  BESKID_SETTLE_SETTLING,      /* request attached; a cancel waits for the packet */
  BESKID_SETTLE_ORPHANED       /* request detached; packet only frees native storage */
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

/* Runtime cancel on an attached operation always settles; it never orphans, because
 * only the packet holds the final status. Native close orphans. Other states are final. */
static inline int BeskidSettleOnCancel(int state) {
  return state == BESKID_SETTLE_IN_FLIGHT ? BESKID_SETTLE_SETTLING : state;
}
static inline int BeskidSettleOnClose(int state) {
  return state == BESKID_SETTLE_IN_FLIGHT ? BESKID_SETTLE_ORPHANED : state;
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
