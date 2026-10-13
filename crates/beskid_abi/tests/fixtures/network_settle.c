/* Platform-independent state-machine test for IOCP completion settlement
 * (assembly/common/network_settle.h). It replays the deadline/completion orderings
 * that the Windows transport must resolve, without a completion port. */
#include <stdio.h>
#include "../../assembly/common/network_settle.h"

static int failures;
#define CHECK(condition, label) do { if (!(condition)) { fprintf(stderr, "FAIL %s\n", label); ++failures; } } while (0)

/* Minimal model of one overlapped operation and its single completion packet. */
struct Model {
  int state;
  int requestAttached;
  int packetQueued;
  uint32_t packetError;
  size_t packetBytes;
  int finalKnown;
  struct BeskidSettleOutcome final;
  int posted, woken;
};

static void ModelDequeue(struct Model *model, uintptr_t operation, size_t capacity) {
  if (!model->packetQueued) return;
  model->packetQueued = 0;
  int delivery = BeskidSettleDelivery(model->state);
  if (!model->requestAttached || delivery == BESKID_SETTLE_DISCARD) return;
  model->final = BeskidSettleClassify(operation, model->packetError, model->packetBytes, capacity);
  model->finalKnown = 1;
  model->requestAttached = 0;
  if (delivery == BESKID_SETTLE_DELIVER_AND_POST) ++model->posted; else ++model->woken;
}

/* Runtime cancel: settle, then the reactor delivers the queued packet. Returns the
 * deadline verdict NetworkFinish reaches when the timeout won the one-winner wait. */
static int ModelDeadlineRace(struct Model *model, uintptr_t operation, size_t capacity, int cancelled, uint32_t cancelError) {
  if (model->requestAttached) {
    model->state = BeskidSettleOnCancel(model->state);
    if (!BeskidSettleCancelAccepted(cancelled, cancelError)) return -1;
    /* The settling cancel waits; the reactor thread dequeues meanwhile. */
    ModelDequeue(model, operation, capacity);
    if (model->requestAttached) return -2; /* a settle must never return before delivery */
  }
  return BeskidSettleTransferWinsDeadline(1, model->final);
}

/* Fake clock and condition variable for BeskidSettleAwait. The packet is dequeued by
 * the "reactor" at deliverAt (milliseconds); a wait wakes at the earlier of its timeout,
 * the delivery, or a spurious wake every spuriousEvery milliseconds. */
struct FakeWait {
  uint64_t now, deliverAt, spuriousEvery;
  int delivered, waits;
  uint32_t longestWait;
};
static uint64_t FakeNow(void *value) { return ((struct FakeWait *)value)->now; }
static int FakeSettled(void *value) {
  struct FakeWait *fake = value;
  if (fake->now >= fake->deliverAt) fake->delivered = 1;
  return fake->delivered;
}
static void FakeSleep(void *value, uint32_t milliseconds) {
  struct FakeWait *fake = value;
  ++fake->waits;
  if (milliseconds > fake->longestWait) fake->longestWait = milliseconds;
  uint64_t wake = fake->now + milliseconds;
  if (fake->deliverAt > fake->now && fake->deliverAt < wake) wake = fake->deliverAt;
  if (fake->spuriousEvery && fake->now + fake->spuriousEvery < wake) wake = fake->now + fake->spuriousEvery;
  fake->now = wake;
}
static int FakeAwait(struct FakeWait *fake, uint32_t bound) {
  struct BeskidSettleWait wait = {fake, FakeNow, FakeSettled, FakeSleep};
  return BeskidSettleAwait(&wait, bound);
}

/* Model of an operation that also tracks its socket slot and the socket poison flag,
 * for the abandoned (settle bound expired) path. */
struct SlotModel {
  int state, requestAttached, slotted, socketOpen, poisoned, packetQueued;
  int requestStatusCancelled;
};
static void SlotExpire(struct SlotModel *model) {
  model->state = BeskidSettleOnExpire(model->state);
  model->requestAttached = 0; model->requestStatusCancelled = 1;
}
static void SlotDequeue(struct SlotModel *model, uintptr_t operation, uint32_t error, size_t bytes, size_t capacity) {
  model->packetQueued = 0;
  if (BeskidSettleDelivery(model->state) == BESKID_SETTLE_DISCARD && model->state == BESKID_SETTLE_ABANDONED && model->slotted) {
    if (BeskidSettleAbandonedPoisons(operation, BeskidSettleClassify(operation, error, bytes, capacity))) model->poisoned = 1;
  }
  model->slotted = 0;
}

int main(void) {
  /* 1. The 0.5.3 gap: the receive completed in the kernel and its packet is still queued
   *    when the deadline wins. CancelIoEx finds nothing (ERROR_NOT_FOUND). The packet
   *    reports success, so the byte is delivered instead of TimedOut. */
  struct Model queued = {BESKID_SETTLE_IN_FLIGHT, 1, 1, 0, 1, 0, {0, 0}, 0, 0};
  CHECK(ModelDeadlineRace(&queued, BESKID_SETTLE_READ, 1, 0, BESKID_SETTLE_ERROR_NOT_FOUND) == 1, "queued completion wins over deadline");
  CHECK(queued.final.kind == BESKID_SETTLE_TRANSFERRED && queued.final.count == 1, "queued completion delivers its byte");
  CHECK(queued.woken == 1 && queued.posted == 0, "settled packet wakes the cancel and posts nothing");
  CHECK(queued.state == BESKID_SETTLE_SETTLING, "runtime cancel settles");

  /* 2. Nothing arrived: CancelIoEx takes effect and the packet reports abort. */
  struct Model idle = {BESKID_SETTLE_IN_FLIGHT, 1, 1, BESKID_SETTLE_ERROR_OPERATION_ABORTED, 0, 0, {0, 0}, 0, 0};
  CHECK(ModelDeadlineRace(&idle, BESKID_SETTLE_READ, 16, 1, 0) == 0, "aborted operation keeps the timeout");
  CHECK(idle.final.kind == BESKID_SETTLE_ABORTED, "abort classification");

  /* 3. The reactor dequeued the packet before cancel: delivered and posted, cancel is a no-op. */
  struct Model early = {BESKID_SETTLE_IN_FLIGHT, 1, 1, 0, 4, 0, {0, 0}, 0, 0};
  ModelDequeue(&early, BESKID_SETTLE_WRITE, 4);
  CHECK(early.posted == 1 && early.woken == 0, "in-flight delivery posts READY");
  CHECK(ModelDeadlineRace(&early, BESKID_SETTLE_WRITE, 4, 0, 0) == 1, "already delivered completion wins over deadline");
  CHECK(early.state == BESKID_SETTLE_IN_FLIGHT, "cancel after delivery does not change state");

  /* 4. Native close orphans: the packet frees storage only, nothing is delivered. */
  struct Model closed = {BESKID_SETTLE_IN_FLIGHT, 1, 1, 0, 8, 0, {0, 0}, 0, 0};
  closed.state = BeskidSettleOnClose(closed.state); closed.requestAttached = 0;
  ModelDequeue(&closed, BESKID_SETTLE_READ, 8);
  CHECK(closed.state == BESKID_SETTLE_ORPHANED && !closed.finalKnown && !closed.posted && !closed.woken, "orphaned packet is discarded");
  CHECK(BeskidSettleDelivery(BESKID_SETTLE_ORPHANED) == BESKID_SETTLE_DISCARD, "orphan delivery");

  /* 5. Close during a settle leaves the operation settling (its cancel still waits for
   *    the packet the closed handle forces), and a second cancel keeps settling. */
  CHECK(BeskidSettleOnClose(BESKID_SETTLE_SETTLING) == BESKID_SETTLE_SETTLING, "close keeps settling state");
  CHECK(BeskidSettleOnCancel(BESKID_SETTLE_SETTLING) == BESKID_SETTLE_SETTLING, "repeated cancel keeps settling state");
  CHECK(BeskidSettleOnCancel(BESKID_SETTLE_ORPHANED) == BESKID_SETTLE_ORPHANED, "cancel keeps orphaned state");

  /* 6. CancelIoEx result handling. */
  CHECK(BeskidSettleCancelAccepted(1, 0), "cancel requested");
  CHECK(BeskidSettleCancelAccepted(0, BESKID_SETTLE_ERROR_NOT_FOUND), "already completed");
  CHECK(!BeskidSettleCancelAccepted(0, 6u /* ERROR_INVALID_HANDLE */), "invalid handle fails closed");
  struct Model corrupt = {BESKID_SETTLE_IN_FLIGHT, 1, 1, 0, 1, 0, {0, 0}, 0, 0};
  CHECK(ModelDeadlineRace(&corrupt, BESKID_SETTLE_READ, 1, 0, 6u) == -1, "unexpected cancel failure is rejected");

  /* 7. Classification table. */
  struct BeskidSettleOutcome outcome;
  outcome = BeskidSettleClassify(BESKID_SETTLE_READ, BESKID_SETTLE_ERROR_OPERATION_ABORTED, 3, 8);
  CHECK(outcome.kind == BESKID_SETTLE_TRANSFERRED && outcome.count == 3, "aborted stream read with bytes is a transfer");
  outcome = BeskidSettleClassify(BESKID_SETTLE_WRITE, BESKID_SETTLE_ERROR_OPERATION_ABORTED, 2, 8);
  CHECK(outcome.kind == BESKID_SETTLE_TRANSFERRED && outcome.count == 2, "aborted stream write with bytes is a short write");
  outcome = BeskidSettleClassify(BESKID_SETTLE_RECEIVE, BESKID_SETTLE_ERROR_OPERATION_ABORTED, 3, 8);
  CHECK(outcome.kind == BESKID_SETTLE_ABORTED, "aborted datagram receive is atomic");
  outcome = BeskidSettleClassify(BESKID_SETTLE_SEND, BESKID_SETTLE_ERROR_OPERATION_ABORTED, 3, 8);
  CHECK(outcome.kind == BESKID_SETTLE_ABORTED, "aborted datagram send is atomic");
  outcome = BeskidSettleClassify(BESKID_SETTLE_ACCEPT, BESKID_SETTLE_ERROR_OPERATION_ABORTED, 0, 0);
  CHECK(outcome.kind == BESKID_SETTLE_ABORTED, "aborted accept");
  outcome = BeskidSettleClassify(BESKID_SETTLE_ACCEPT, 0, 0, 0);
  CHECK(outcome.kind == BESKID_SETTLE_TRANSFERRED && BeskidSettleCompleted(outcome), "completed accept wins");
  outcome = BeskidSettleClassify(BESKID_SETTLE_CONNECT, 0, 0, 0);
  CHECK(BeskidSettleTransferWinsDeadline(1, outcome), "completed connect wins over deadline");
  outcome = BeskidSettleClassify(BESKID_SETTLE_READ, 0, 0, 8);
  CHECK(outcome.kind == BESKID_SETTLE_TRANSFERRED && outcome.count == 0, "end of stream is a completed read");
  outcome = BeskidSettleClassify(BESKID_SETTLE_RECEIVE, BESKID_SETTLE_WSAEMSGSIZE, 0, 5);
  CHECK(outcome.kind == BESKID_SETTLE_TRUNCATED && outcome.count == 5 && BeskidSettleCompleted(outcome), "truncated datagram");
  outcome = BeskidSettleClassify(BESKID_SETTLE_RECEIVE, BESKID_SETTLE_ERROR_MORE_DATA, 0, 5);
  CHECK(outcome.kind == BESKID_SETTLE_TRUNCATED, "more-data datagram");
  outcome = BeskidSettleClassify(BESKID_SETTLE_READ, BESKID_SETTLE_WSAEMSGSIZE, 0, 5);
  CHECK(outcome.kind == BESKID_SETTLE_FAILED, "message size on a stream is a failure");
  outcome = BeskidSettleClassify(BESKID_SETTLE_READ, BESKID_SETTLE_ERROR_NETNAME_DELETED, 0, 5);
  CHECK(outcome.kind == BESKID_SETTLE_FAILED && !BeskidSettleTransferWinsDeadline(1, outcome), "reset keeps the timeout");
  outcome = BeskidSettleClassify(BESKID_SETTLE_READ, 0, 64, 16);
  CHECK(outcome.count == 16, "byte count is bounded by capacity");

  /* 8. Only a timeout winner is converted; cancellation and close winners are unchanged. */
  outcome = BeskidSettleClassify(BESKID_SETTLE_READ, 0, 1, 1);
  CHECK(!BeskidSettleTransferWinsDeadline(0, outcome), "non-timeout winner is unchanged");

  /* 9. Bounded settle wait: the packet arrives in time, through spurious wakes. */
  struct FakeWait prompt = {1000, 1003, 1, 0, 0, 0};
  CHECK(FakeAwait(&prompt, BESKID_SETTLE_WAIT_MS) == 1 && prompt.now == 1003, "packet within the bound settles");
  struct FakeWait already = {50, 0, 0, 0, 0, 0};
  CHECK(FakeAwait(&already, BESKID_SETTLE_WAIT_MS) == 1 && already.waits == 0, "already delivered packet needs no wait");
  /* The packet never arrives: the wait ends exactly at the bound, never later, also with
   * frequent spurious wakes, and no single wait exceeds the remaining time. */
  struct FakeWait never = {7, UINT64_MAX, 0, 0, 0, 0};
  CHECK(FakeAwait(&never, BESKID_SETTLE_WAIT_MS) == 0 && never.now == 7 + BESKID_SETTLE_WAIT_MS, "missing packet expires at the bound");
  CHECK(never.longestWait <= BESKID_SETTLE_WAIT_MS, "one wait never exceeds the bound");
  struct FakeWait noisy = {0, UINT64_MAX, 3, 0, 0, 0};
  CHECK(FakeAwait(&noisy, BESKID_SETTLE_WAIT_MS) == 0 && noisy.now == BESKID_SETTLE_WAIT_MS, "spurious wakes do not extend the bound");
  /* The packet arrives exactly at the bound: the last test sees it, so it settles. */
  struct FakeWait edge = {0, BESKID_SETTLE_WAIT_MS, 0, 0, 0, 0};
  CHECK(FakeAwait(&edge, BESKID_SETTLE_WAIT_MS) == 1, "delivery at the bound settles");
  CHECK(BESKID_SETTLE_WAIT_MS > 0 && BESKID_SETTLE_DRAIN_MS >= BESKID_SETTLE_WAIT_MS, "bounds are ordered");

  /* 10. The bound expires before the packet: the operation is abandoned. The caller gets
   *     a cancelled status at once (the deadline stands), the operation keeps its slot
   *     (the direction is busy), and an aborted late packet frees the slot without poison. */
  struct SlotModel late = {BESKID_SETTLE_IN_FLIGHT, 1, 1, 1, 0, 1, 0};
  late.state = BeskidSettleOnCancel(late.state);
  SlotExpire(&late);
  CHECK(late.state == BESKID_SETTLE_ABANDONED && !late.requestAttached && late.requestStatusCancelled, "expired settle abandons");
  CHECK(late.slotted && late.packetQueued, "abandoned operation keeps its slot and storage until the packet");
  CHECK(BeskidSettleDelivery(BESKID_SETTLE_ABANDONED) == BESKID_SETTLE_DISCARD, "abandoned packet is not delivered");
  CHECK(BeskidSettleOnCancel(BESKID_SETTLE_ABANDONED) == BESKID_SETTLE_ABANDONED, "cancel keeps abandoned state");
  CHECK(BeskidSettleOnExpire(BESKID_SETTLE_IN_FLIGHT) == BESKID_SETTLE_IN_FLIGHT, "only a settling operation expires");
  SlotDequeue(&late, BESKID_SETTLE_READ, BESKID_SETTLE_ERROR_OPERATION_ABORTED, 0, 8);
  CHECK(!late.slotted && !late.poisoned, "aborted late packet releases the slot, stream intact");

  /* 11. A late packet that moved stream bytes poisons the socket. */
  struct SlotModel lateRead = {BESKID_SETTLE_SETTLING, 1, 1, 1, 0, 1, 0};
  SlotExpire(&lateRead);
  SlotDequeue(&lateRead, BESKID_SETTLE_READ, 0, 3, 8);
  CHECK(lateRead.poisoned && !lateRead.slotted, "late stream read poisons");
  struct SlotModel lateAbortedWrite = {BESKID_SETTLE_SETTLING, 1, 1, 1, 0, 1, 0};
  SlotExpire(&lateAbortedWrite);
  SlotDequeue(&lateAbortedWrite, BESKID_SETTLE_WRITE, BESKID_SETTLE_ERROR_OPERATION_ABORTED, 2, 8);
  CHECK(lateAbortedWrite.poisoned, "partially sent late write poisons");
  struct SlotModel lateConnect = {BESKID_SETTLE_SETTLING, 1, 1, 1, 0, 1, 0};
  SlotExpire(&lateConnect);
  SlotDequeue(&lateConnect, BESKID_SETTLE_CONNECT, 0, 0, 0);
  CHECK(lateConnect.poisoned, "late established connect poisons");
  CHECK(!BeskidSettleAbandonedPoisons(BESKID_SETTLE_READ, BeskidSettleClassify(BESKID_SETTLE_READ, 0, 0, 8)), "late end of stream keeps the stream");
  CHECK(!BeskidSettleAbandonedPoisons(BESKID_SETTLE_RECEIVE, BeskidSettleClassify(BESKID_SETTLE_RECEIVE, 0, 9, 16)), "late datagram is loss, not poison");
  CHECK(!BeskidSettleAbandonedPoisons(BESKID_SETTLE_SEND, BeskidSettleClassify(BESKID_SETTLE_SEND, 0, 9, 16)), "late datagram send is not poison");
  CHECK(!BeskidSettleAbandonedPoisons(BESKID_SETTLE_ACCEPT, BeskidSettleClassify(BESKID_SETTLE_ACCEPT, 0, 0, 0)), "late accept is closed, listener intact");
  CHECK(!BeskidSettleAbandonedPoisons(BESKID_SETTLE_READ, BeskidSettleClassify(BESKID_SETTLE_READ, BESKID_SETTLE_ERROR_NETNAME_DELETED, 0, 8)), "late failure is not poison");

  /* 12. Native close of an abandoned operation orphans it; its packet only frees storage
   *     and cannot poison the freed socket. */
  struct SlotModel closedLate = {BESKID_SETTLE_SETTLING, 1, 1, 1, 0, 1, 0};
  SlotExpire(&closedLate);
  closedLate.state = BeskidSettleOnClose(closedLate.state); closedLate.slotted = 0; closedLate.socketOpen = 0;
  SlotDequeue(&closedLate, BESKID_SETTLE_READ, 0, 5, 8);
  CHECK(closedLate.state == BESKID_SETTLE_ORPHANED && !closedLate.poisoned, "closed abandoned operation is orphaned");

  if (failures) return 1;
  printf("settle=ok\n");
  return 0;
}
