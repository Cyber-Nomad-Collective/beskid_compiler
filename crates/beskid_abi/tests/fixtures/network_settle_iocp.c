/* Windows-only witness for the completion-port facts that network_settle.h relies on.
 * It drives a real loopback TCP pair through an I/O completion port:
 *   A. a receive completes in the kernel and its packet is still queued when the
 *      deadline wins: CancelIoEx fails with ERROR_NOT_FOUND and the dequeued packet
 *      reports success with the byte, which the settlement rules deliver;
 *   B. a receive with no data is cancelled: the packet reports ERROR_OPERATION_ABORTED
 *      with zero bytes, and the stream data sent afterwards is not consumed by it;
 *   C. the bounded settle wait (BeskidSettleAwait over GetTickCount64 and an SRW
 *      condition variable, as in network_iocp.h) expires when nobody dequeues the
 *      packet, within the bound plus timer slack; the abandoned operation's packet is
 *      still queued afterwards, its OVERLAPPED and buffer intact, and a late transfer
 *      is recognised as stream poison;
 *   D. closesocket on a socket with a pending receive forces its packet, which is what
 *      the bounded shutdown drain relies on.
 * Run on Windows (Woodpecker windows pipeline runs `cargo test --workspace`):
 *   cd compiler && cargo test --locked -p beskid_abi --test network_iocp_settle */
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <stdio.h>
#include <string.h>
#include "../../assembly/common/network_settle.h"

#define CHECK(condition, code, label) do { if (!(condition)) { fprintf(stderr, "FAIL %s (error %lu)\n", label, (unsigned long)GetLastError()); return code; } } while (0)

static int WaitCompleted(OVERLAPPED *overlapped) {
  for (int attempt = 0; attempt < 5000; ++attempt) {
    if (HasOverlappedIoCompleted(overlapped)) return 1;
    Sleep(1);
  }
  return 0;
}

static SRWLOCK settleLock = SRWLOCK_INIT;
static CONDITION_VARIABLE settleCondition = CONDITION_VARIABLE_INIT;
static uint64_t SettleNow(void *context) { (void)context; return GetTickCount64(); }
static int SettleNever(void *context) { return *(volatile int *)context; }
static void SettleSleep(void *context, uint32_t milliseconds) {
  (void)context;
  SleepConditionVariableSRW(&settleCondition, &settleLock, milliseconds, 0);
}

int main(void) {
  WSADATA data;
  CHECK(WSAStartup(MAKEWORD(2, 2), &data) == 0, 10, "WSAStartup");
  SOCKET listener = WSASocketW(AF_INET, SOCK_STREAM, 0, NULL, 0, WSA_FLAG_OVERLAPPED);
  CHECK(listener != INVALID_SOCKET, 11, "listener socket");
  struct sockaddr_in address;
  memset(&address, 0, sizeof(address));
  address.sin_family = AF_INET; address.sin_addr.s_addr = htonl(INADDR_LOOPBACK); address.sin_port = 0;
  CHECK(bind(listener, (struct sockaddr *)&address, sizeof(address)) == 0, 12, "bind");
  CHECK(listen(listener, 1) == 0, 13, "listen");
  int length = sizeof(address);
  CHECK(getsockname(listener, (struct sockaddr *)&address, &length) == 0, 14, "getsockname");
  SOCKET client = WSASocketW(AF_INET, SOCK_STREAM, 0, NULL, 0, WSA_FLAG_OVERLAPPED);
  CHECK(client != INVALID_SOCKET, 15, "client socket");
  CHECK(connect(client, (struct sockaddr *)&address, sizeof(address)) == 0, 16, "connect");
  SOCKET server = accept(listener, NULL, NULL);
  CHECK(server != INVALID_SOCKET, 17, "accept");
  HANDLE port = CreateIoCompletionPort(INVALID_HANDLE_VALUE, NULL, 0, 1);
  CHECK(port != NULL, 18, "completion port");
  CHECK(CreateIoCompletionPort((HANDLE)server, port, 1, 0) == port, 19, "associate");

  /* A. Completed in the kernel, packet queued, deadline wins, then settle. */
  char byte = 0;
  OVERLAPPED overlapped;
  memset(&overlapped, 0, sizeof(overlapped));
  WSABUF buffer = {1, &byte};
  DWORD bytes = 0, flags = 0;
  int result = WSARecv(server, &buffer, 1, &bytes, &flags, &overlapped, NULL);
  CHECK(result == SOCKET_ERROR && WSAGetLastError() == WSA_IO_PENDING, 20, "receive A is pending");
  CHECK(send(client, "x", 1, 0) == 1, 21, "send A");
  CHECK(WaitCompleted(&overlapped), 22, "receive A completes in the kernel");
  BOOL cancelled = CancelIoEx((HANDLE)server, &overlapped);
  DWORD cancelError = cancelled ? 0 : GetLastError();
  CHECK(!cancelled && cancelError == ERROR_NOT_FOUND, 23, "CancelIoEx finds no pending operation after completion");
  CHECK(BeskidSettleCancelAccepted(cancelled != 0, (uint32_t)cancelError), 24, "settlement accepts ERROR_NOT_FOUND");
  ULONG_PTR key = 0; OVERLAPPED *dequeued = NULL; bytes = 0;
  BOOL ok = GetQueuedCompletionStatus(port, &bytes, &key, &dequeued, 5000);
  DWORD error = ok ? 0 : GetLastError();
  CHECK(dequeued == &overlapped, 25, "packet A is still queued after CancelIoEx");
  struct BeskidSettleOutcome outcome = BeskidSettleClassify(BESKID_SETTLE_READ, (uint32_t)error, bytes, 1);
  CHECK(outcome.kind == BESKID_SETTLE_TRANSFERRED && outcome.count == 1 && byte == 'x', 26, "packet A reports the transfer");
  CHECK(BeskidSettleTransferWinsDeadline(1, outcome), 27, "completed transfer wins over the deadline");

  /* B. Nothing arrived: the cancel takes effect and consumes no stream data. */
  byte = 0; bytes = 0; flags = 0;
  memset(&overlapped, 0, sizeof(overlapped));
  result = WSARecv(server, &buffer, 1, &bytes, &flags, &overlapped, NULL);
  CHECK(result == SOCKET_ERROR && WSAGetLastError() == WSA_IO_PENDING, 30, "receive B is pending");
  CHECK(CancelIoEx((HANDLE)server, &overlapped), 31, "CancelIoEx accepts the pending receive");
  dequeued = NULL; bytes = 0;
  ok = GetQueuedCompletionStatus(port, &bytes, &key, &dequeued, 5000);
  error = ok ? 0 : GetLastError();
  CHECK(dequeued == &overlapped, 32, "cancelled receive still queues its packet");
  CHECK(error == ERROR_OPERATION_ABORTED && bytes == 0, 33, "cancelled receive reports abort");
  outcome = BeskidSettleClassify(BESKID_SETTLE_READ, (uint32_t)error, bytes, 1);
  CHECK(outcome.kind == BESKID_SETTLE_ABORTED && !BeskidSettleTransferWinsDeadline(1, outcome), 34, "abort keeps the timeout");
  CHECK(send(client, "y", 1, 0) == 1, 35, "send B");
  memset(&overlapped, 0, sizeof(overlapped)); bytes = 0; flags = 0;
  result = WSARecv(server, &buffer, 1, &bytes, &flags, &overlapped, NULL);
  CHECK(result == 0 || WSAGetLastError() == WSA_IO_PENDING, 36, "follow-up receive starts");
  dequeued = NULL;
  ok = GetQueuedCompletionStatus(port, &bytes, &key, &dequeued, 5000);
  CHECK(ok && dequeued == &overlapped && bytes == 1 && byte == 'y', 37, "cancelled receive consumed no data");

  /* C. Bounded settle: data arrives, the receive completes in the kernel, the cancel is
   *    issued, and no reactor dequeues. The wait expires within the bound; the packet
   *    is still queued and reports the transfer, which an abandoned operation turns
   *    into stream poison. The OVERLAPPED and buffer were never released meanwhile. */
  char late = 0;
  WSABUF lateBuffer = {1, &late};
  memset(&overlapped, 0, sizeof(overlapped)); bytes = 0; flags = 0;
  result = WSARecv(server, &lateBuffer, 1, &bytes, &flags, &overlapped, NULL);
  CHECK(result == SOCKET_ERROR && WSAGetLastError() == WSA_IO_PENDING, 40, "receive C is pending");
  CHECK(send(client, "z", 1, 0) == 1, 41, "send C");
  CHECK(WaitCompleted(&overlapped), 42, "receive C completes in the kernel");
  cancelled = CancelIoEx((HANDLE)server, &overlapped);
  CHECK(BeskidSettleCancelAccepted(cancelled != 0, cancelled ? 0u : (uint32_t)GetLastError()), 43, "cancel C accepted");
  int delivered = 0;
  struct BeskidSettleWait wait = {&delivered, SettleNow, SettleNever, SettleSleep};
  AcquireSRWLockExclusive(&settleLock);
  uint64_t start = GetTickCount64();
  int settled = BeskidSettleAwait(&wait, BESKID_SETTLE_WAIT_MS);
  uint64_t elapsed = GetTickCount64() - start;
  ReleaseSRWLockExclusive(&settleLock);
  CHECK(!settled, 44, "settle wait expires without a reactor");
  CHECK(elapsed >= BESKID_SETTLE_WAIT_MS && elapsed < BESKID_SETTLE_WAIT_MS + 1000u, 45, "settle wait is bounded");
  dequeued = NULL; bytes = 0;
  ok = GetQueuedCompletionStatus(port, &bytes, &key, &dequeued, 5000);
  error = ok ? 0 : GetLastError();
  CHECK(dequeued == &overlapped && late == 'z', 46, "abandoned packet is still queued with its transfer");
  outcome = BeskidSettleClassify(BESKID_SETTLE_READ, (uint32_t)error, bytes, 1);
  CHECK(BeskidSettleAbandonedPoisons(BESKID_SETTLE_READ, outcome), 47, "late stream transfer poisons");

  /* D. Closing the handle forces the packet of a pending receive (shutdown drain). */
  memset(&overlapped, 0, sizeof(overlapped)); bytes = 0; flags = 0;
  result = WSARecv(server, &lateBuffer, 1, &bytes, &flags, &overlapped, NULL);
  CHECK(result == SOCKET_ERROR && WSAGetLastError() == WSA_IO_PENDING, 50, "receive D is pending");
  CHECK(closesocket(server) == 0, 51, "close D");
  dequeued = NULL; bytes = 0;
  ok = GetQueuedCompletionStatus(port, &bytes, &key, &dequeued, BESKID_SETTLE_DRAIN_MS);
  CHECK(!ok && dequeued == &overlapped && bytes == 0, 52, "closed handle forces the packet within the drain bound");
  outcome = BeskidSettleClassify(BESKID_SETTLE_READ, (uint32_t)GetLastError(), bytes, 1);
  CHECK(!BeskidSettleCompleted(outcome), 53, "forced packet reports no transfer");

  closesocket(client); closesocket(listener); CloseHandle(port); WSACleanup();
  printf("iocp-settle=ok\n");
  return 0;
}
