/* Windows-only witness for the completion-port facts that network_settle.h relies on.
 * It drives a real loopback TCP pair through an I/O completion port:
 *   A. a receive completes in the kernel and its packet is still queued when the
 *      deadline wins: CancelIoEx fails with ERROR_NOT_FOUND and the dequeued packet
 *      reports success with the byte, which the settlement rules deliver;
 *   B. a receive with no data is cancelled: the packet reports ERROR_OPERATION_ABORTED
 *      with zero bytes, and the stream data sent afterwards is not consumed by it. */
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

  closesocket(server); closesocket(client); closesocket(listener); CloseHandle(port); WSACleanup();
  printf("iocp-settle=ok\n");
  return 0;
}
