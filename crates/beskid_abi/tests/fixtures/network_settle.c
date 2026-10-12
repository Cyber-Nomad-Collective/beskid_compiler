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

  /* 5. A settling operation is no longer in a socket slot: close cannot orphan it,
   *    and a second cancel keeps settling. */
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

  if (failures) return 1;
  printf("settle=ok\n");
  return 0;
}
