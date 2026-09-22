#include "../common/network.h"

struct BeskidNetworkReactor;
struct BeskidNetworkOperation;
struct BeskidNativeSocket {
  struct BeskidNativeSocket *next;
  struct BeskidNetworkReactor *reactor;
  SOCKET socket;
  int family, kind;
  struct BeskidNetworkOperation *read, *write;
};
struct BeskidNetworkOperation {
  OVERLAPPED overlapped;
  struct BeskidNetworkOperation *next;
  struct BeskidNetworkReactor *reactor;
  struct BeskidNativeSocket *socket;
  struct BeskidNetworkRequest *request;
  SOCKET descriptor, accepted;
  uintptr_t operation;
  void *buffer;
  size_t capacity;
  struct sockaddr_storage address;
  int address_length;
  DWORD flags;
  char accept_addresses[2 * (sizeof(struct sockaddr_storage) + 16)];
};
struct BeskidNetworkReactor {
  HANDLE port, thread;
  int stopping;
  size_t pending;
  struct BeskidNativeSocket *sockets;
  struct BeskidNetworkOperation *operations;
};
static SRWLOCK beskid_network_lock = SRWLOCK_INIT;
static SRWLOCK beskid_network_table_lock = SRWLOCK_INIT;
static SRWLOCK beskid_network_startup_lock = SRWLOCK_INIT;
static uintptr_t beskid_network_table;
static int beskid_network_started;
static int BeskidNetworkPlatformReady(void) {
  AcquireSRWLockExclusive(&beskid_network_startup_lock);
  if (!beskid_network_started) {
    WSADATA data;
    if (WSAStartup(MAKEWORD(2, 2), &data)) { ReleaseSRWLockExclusive(&beskid_network_startup_lock); return NET_NETWORK_DOWN; }
    beskid_network_started = 1;
  }
  ReleaseSRWLockExclusive(&beskid_network_startup_lock); return NET_OK;
}
void beskid_rt_v5_intrinsic_network_table_lock(void) { AcquireSRWLockExclusive(&beskid_network_table_lock); }
void beskid_rt_v5_intrinsic_network_table_unlock(void) { ReleaseSRWLockExclusive(&beskid_network_table_lock); }
void *beskid_rt_v5_intrinsic_network_table_root(void) { return &beskid_network_table; }
static struct BeskidNativeSocket *BeskidNetworkWrap(SOCKET descriptor, int family, int kind) {
  struct BeskidNativeSocket *socket = BeskidNetworkAllocate(sizeof(*socket));
  if (!socket) { closesocket(descriptor); return NULL; }
  socket->socket = descriptor; socket->family = family; socket->kind = kind;
  return socket;
}
int32_t beskid_rt_v5_intrinsic_network_open(int32_t kind, int32_t family, void *output) {
  if (!output || (kind != 1 && kind != 2) || (family != 4 && family != 6)) return NET_INVALID_ADDRESS;
  *(void **)output = NULL;
  int status = BeskidNetworkPlatformReady(); if (status) return status;
  SOCKET descriptor = WSASocketW(family == 4 ? AF_INET : AF_INET6, kind == 1 ? SOCK_STREAM : SOCK_DGRAM, 0, NULL, 0, WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT);
  if (descriptor == INVALID_SOCKET) return BeskidNetworkError(WSAGetLastError());
  struct BeskidNativeSocket *socket = BeskidNetworkWrap(descriptor, family, kind);
  if (!socket) return NET_RESOURCE_EXHAUSTED;
  *(void **)output = socket; return NET_OK;
}
static int BeskidNetworkExtension(SOCKET socket, GUID guid, void *function, DWORD size) {
  DWORD count;
  return WSAIoctl(socket, SIO_GET_EXTENSION_FUNCTION_POINTER, &guid, sizeof(guid), function, size, &count, NULL, NULL) ? BeskidNetworkError(WSAGetLastError()) : NET_OK;
}
static void BeskidNetworkOperationFree(struct BeskidNetworkOperation *operation) {
  if (operation->accepted != INVALID_SOCKET) closesocket(operation->accepted);
  BeskidNetworkFree(operation->buffer, operation->capacity);
  BeskidNetworkFree(operation, sizeof(*operation));
}
static void BeskidNetworkDetach(struct BeskidNetworkOperation *operation) {
  if (operation->socket) {
    if (operation->socket->read == operation) operation->socket->read = NULL;
    if (operation->socket->write == operation) operation->socket->write = NULL;
  }
  if (operation->request) operation->request->native_state = NULL;
  operation->request = NULL; operation->socket = NULL;
}
static void BeskidNetworkCancelLocked(struct BeskidNetworkOperation *operation) {
  if (!operation) return;
  if (operation->request) operation->request->status = NET_CANCELLED;
  BeskidNetworkDetach(operation);
  /* OVERLAPPED and buffers survive until the completion is removed from IOCP. */
  CancelIoEx((HANDLE)operation->descriptor, &operation->overlapped);
}
int32_t beskid_rt_v5_intrinsic_network_submit(void *reactor_value, void *socket_value, void *request_value) {
  struct BeskidNetworkReactor *reactor = reactor_value;
  struct BeskidNativeSocket *socket = socket_value;
  struct BeskidNetworkRequest *request = request_value;
  if (!reactor || !socket || !request) return NET_CLOSED;
  AcquireSRWLockExclusive(&beskid_network_lock);
  int status = NET_CLOSED;
  if (reactor->stopping || (socket->reactor && socket->reactor != reactor)) goto done;
  if (!socket->reactor) {
    if (!CreateIoCompletionPort((HANDLE)socket->socket, reactor->port, 1, 0)) { status = NET_RESOURCE_EXHAUSTED; goto done; }
    socket->reactor = reactor; socket->next = reactor->sockets; reactor->sockets = socket;
  }
  struct BeskidNetworkOperation **slot = (request->operation == NET_ACCEPT || request->operation == NET_READ || request->operation == NET_RECEIVE) ? &socket->read : &socket->write;
  if (*slot) { status = NET_BUSY; goto done; }
  if (request->length > ULONG_MAX) { status = NET_MESSAGE_TOO_LARGE; goto done; }
  request->count = 0; request->truncated = 0; request->accepted = NULL; request->native_state = NULL;
  struct BeskidNetworkOperation *operation = BeskidNetworkAllocate(sizeof(*operation));
  if (!operation) { status = NET_RESOURCE_EXHAUSTED; goto done; }
  operation->reactor = reactor; operation->socket = socket; operation->request = request;
  operation->operation = request->operation; operation->descriptor = socket->socket; operation->accepted = INVALID_SOCKET;
  operation->capacity = request->length; operation->buffer = BeskidNetworkAllocate(operation->capacity);
  if (!operation->buffer) { status = NET_RESOURCE_EXHAUSTED; goto failed; }
  if ((request->operation == NET_WRITE || request->operation == NET_SEND) && request->length) memcpy(operation->buffer, request->buffer, request->length);
  if (request->address && (request->operation == NET_CONNECT || request->operation == NET_SEND)) {
    status = BeskidNetworkDecode(request->address, &operation->address, &operation->address_length);
    if (status) goto failed;
  }
  WSABUF buffer = {(ULONG)request->length, operation->buffer};
  DWORD bytes = 0;
  int result = SOCKET_ERROR;
  switch (request->operation) {
    case NET_CONNECT:
      if (socket->kind == 2) {
        result = connect(socket->socket, (struct sockaddr *)&operation->address, operation->address_length);
        status = result ? BeskidNetworkError(WSAGetLastError()) : NET_OK;
        goto failed; /* UDP connect has no pending stream handshake. */
      } else {
        LPFN_CONNECTEX connect_ex;
        GUID guid = WSAID_CONNECTEX;
        status = BeskidNetworkExtension(socket->socket, guid, &connect_ex, sizeof(connect_ex));
        if (status) goto failed;
        struct sockaddr_storage any;
        memset(&any, 0, sizeof(any)); any.ss_family = socket->family == 4 ? AF_INET : AF_INET6;
        if (bind(socket->socket, (struct sockaddr *)&any, socket->family == 4 ? sizeof(struct sockaddr_in) : sizeof(struct sockaddr_in6))) { status = BeskidNetworkError(WSAGetLastError()); goto failed; }
        result = connect_ex(socket->socket, (struct sockaddr *)&operation->address, operation->address_length, NULL, 0, &bytes, &operation->overlapped) ? 0 : SOCKET_ERROR;
      }
      break;
    case NET_ACCEPT: {
      LPFN_ACCEPTEX accept_ex;
      GUID guid = WSAID_ACCEPTEX;
      status = BeskidNetworkExtension(socket->socket, guid, &accept_ex, sizeof(accept_ex));
      if (status) goto failed;
      operation->accepted = WSASocketW(socket->family == 4 ? AF_INET : AF_INET6, SOCK_STREAM, 0, NULL, 0, WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT);
      if (operation->accepted == INVALID_SOCKET) { status = BeskidNetworkError(WSAGetLastError()); goto failed; }
      result = accept_ex(socket->socket, operation->accepted, operation->accept_addresses, 0, sizeof(struct sockaddr_storage) + 16, sizeof(struct sockaddr_storage) + 16, &bytes, &operation->overlapped) ? 0 : SOCKET_ERROR;
      break;
    }
    case NET_READ: result = WSARecv(socket->socket, &buffer, 1, &bytes, &operation->flags, &operation->overlapped, NULL); break;
    case NET_WRITE: result = WSASend(socket->socket, &buffer, 1, &bytes, 0, &operation->overlapped, NULL); break;
    case NET_RECEIVE:
      operation->address_length = sizeof(operation->address);
      result = WSARecvFrom(socket->socket, &buffer, 1, &bytes, &operation->flags, (struct sockaddr *)&operation->address, &operation->address_length, &operation->overlapped, NULL);
      break;
    case NET_SEND:
      result = request->address ? WSASendTo(socket->socket, &buffer, 1, &bytes, 0, (struct sockaddr *)&operation->address, operation->address_length, &operation->overlapped, NULL) : WSASend(socket->socket, &buffer, 1, &bytes, 0, &operation->overlapped, NULL);
      break;
    default: status = NET_UNSUPPORTED; goto failed;
  }
  if (result == SOCKET_ERROR && WSAGetLastError() != WSA_IO_PENDING) {
    int error = WSAGetLastError();
    if (request->operation == NET_RECEIVE && error == WSAEMSGSIZE) {
      /* A synchronous truncated datagram is consumed successfully; Winsock
       * does not enqueue a completion for this immediate error return. */
      request->count = request->length; request->truncated = 1;
      if (request->length) memcpy(request->buffer, operation->buffer, request->length);
      if (request->address) BeskidNetworkEncode((struct sockaddr *)&operation->address, request->address);
      status = NET_OK;
    } else status = BeskidNetworkError(error);
    goto failed;
  }
  /* Even immediate overlapped success queues exactly one completion packet. */
  operation->next = reactor->operations; reactor->operations = operation; ++reactor->pending;
  *slot = operation; request->native_state = operation; status = NET_PENDING; goto done;
failed:
  BeskidNetworkOperationFree(operation);
done:
  request->status = status; ReleaseSRWLockExclusive(&beskid_network_lock); return status;
}
void beskid_rt_v5_intrinsic_network_cancel(void *reactor, void *value) {
  (void)reactor;
  struct BeskidNetworkRequest *request = value;
  if (!request) return;
  AcquireSRWLockExclusive(&beskid_network_lock);
  BeskidNetworkCancelLocked(request->native_state);
  ReleaseSRWLockExclusive(&beskid_network_lock);
}
int32_t beskid_rt_v5_intrinsic_network_reactor_poll(void *value, int32_t timeout) {
  struct BeskidNetworkReactor *reactor = value;
  if (!reactor) return -NET_CLOSED;
  DWORD bytes = 0; ULONG_PTR key = 0; OVERLAPPED *overlapped = NULL;
  BOOL ok = GetQueuedCompletionStatus(reactor->port, &bytes, &key, &overlapped, timeout < 0 ? INFINITE : (DWORD)timeout);
  DWORD error = ok ? 0 : GetLastError();
  if (!overlapped) return error && error != WAIT_TIMEOUT ? -NET_NETWORK_DOWN : 0;
  struct BeskidNetworkOperation *operation = (void *)overlapped;
  AcquireSRWLockExclusive(&beskid_network_lock);
  struct BeskidNetworkOperation **link = &reactor->operations;
  while (*link && *link != operation) link = &(*link)->next;
  if (*link) *link = operation->next;
  --reactor->pending;
  struct BeskidNetworkRequest *request = operation->request;
  if (request) {
    int status = error ? BeskidNetworkError((int)error) : NET_OK;
    if (operation->operation == NET_RECEIVE && (error == WSAEMSGSIZE || error == ERROR_MORE_DATA)) { status = NET_OK; request->truncated = 1; bytes = (DWORD)operation->capacity; }
    if (!status && operation->operation == NET_CONNECT && setsockopt(operation->descriptor, SOL_SOCKET, SO_UPDATE_CONNECT_CONTEXT, NULL, 0)) status = BeskidNetworkError(WSAGetLastError());
    if (!status && operation->operation == NET_ACCEPT) {
      if (setsockopt(operation->accepted, SOL_SOCKET, SO_UPDATE_ACCEPT_CONTEXT, (const char *)&operation->descriptor, sizeof(operation->descriptor))) status = BeskidNetworkError(WSAGetLastError());
      else {
        request->accepted = BeskidNetworkWrap(operation->accepted, operation->socket->family, 1);
        operation->accepted = INVALID_SOCKET;
        if (!request->accepted) status = NET_RESOURCE_EXHAUSTED;
        else if (request->address) {
          struct sockaddr_storage address; int length = sizeof(address);
          if (!getpeername(((struct BeskidNativeSocket *)request->accepted)->socket, (struct sockaddr *)&address, &length)) BeskidNetworkEncode((struct sockaddr *)&address, request->address);
        }
      }
    }
    if (!status && (operation->operation == NET_READ || operation->operation == NET_RECEIVE)) {
      if (bytes > operation->capacity) bytes = (DWORD)operation->capacity;
      if (bytes) memcpy(request->buffer, operation->buffer, bytes);
      if (operation->operation == NET_RECEIVE && request->address) BeskidNetworkEncode((struct sockaddr *)&operation->address, request->address);
    }
    request->count = bytes; request->status = status;
    BeskidNetworkDetach(operation);
    beskid_rt_v5_intrinsic_owner_post(request->owner, request->token, 1);
  }
  BeskidNetworkOperationFree(operation);
  ReleaseSRWLockExclusive(&beskid_network_lock); return 1;
}
static DWORD WINAPI BeskidNetworkReactorMain(LPVOID value) {
  struct BeskidNetworkReactor *reactor = value;
  for (;;) {
    AcquireSRWLockExclusive(&beskid_network_lock);
    int stop = reactor->stopping && reactor->pending == 0;
    ReleaseSRWLockExclusive(&beskid_network_lock);
    if (stop) return 0;
    if (beskid_rt_v5_intrinsic_network_reactor_poll(reactor, -1) < 0) {
      /* A failed completion port is fatal transport corruption, never polling fallback. */
      beskid_rt_v5_trap(10, NULL, 0);
    }
  }
}
void *beskid_rt_v5_intrinsic_network_reactor_create(void) {
  if (BeskidNetworkPlatformReady()) return NULL;
  struct BeskidNetworkReactor *reactor = BeskidNetworkAllocate(sizeof(*reactor));
  if (!reactor) return NULL;
  reactor->port = CreateIoCompletionPort(INVALID_HANDLE_VALUE, NULL, 0, 1);
  if (!reactor->port) { BeskidNetworkFree(reactor, sizeof(*reactor)); return NULL; }
  reactor->thread = CreateThread(NULL, 0, BeskidNetworkReactorMain, reactor, 0, NULL);
  if (!reactor->thread) { CloseHandle(reactor->port); BeskidNetworkFree(reactor, sizeof(*reactor)); return NULL; }
  return reactor;
}
int32_t beskid_rt_v5_intrinsic_network_close(void *value) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_OK;
  AcquireSRWLockExclusive(&beskid_network_lock);
  struct BeskidNetworkOperation *operations[2] = {socket->read, socket->write};
  for (int i = 0; i < 2; ++i) if (operations[i]) {
    struct BeskidNetworkRequest *request = operations[i]->request;
    BeskidNetworkCancelLocked(operations[i]);
    if (request) { request->status = NET_CLOSED; beskid_rt_v5_intrinsic_owner_post(request->owner, request->token, 1); }
  }
  if (socket->reactor) {
    struct BeskidNativeSocket **link = &socket->reactor->sockets;
    while (*link && *link != socket) link = &(*link)->next;
    if (*link) *link = socket->next;
  }
  int status = closesocket(socket->socket) ? NET_CLEANUP_FAILED : NET_OK;
  BeskidNetworkFree(socket, sizeof(*socket)); ReleaseSRWLockExclusive(&beskid_network_lock); return status;
}
void beskid_rt_v5_intrinsic_network_reactor_destroy(void *value) {
  struct BeskidNetworkReactor *reactor = value;
  if (!reactor) return;
  AcquireSRWLockExclusive(&beskid_network_lock); reactor->stopping = 1; ReleaseSRWLockExclusive(&beskid_network_lock);
  while (reactor->sockets) beskid_rt_v5_intrinsic_network_close(reactor->sockets);
  PostQueuedCompletionStatus(reactor->port, 0, 0, NULL);
  WaitForSingleObject(reactor->thread, INFINITE);
  CloseHandle(reactor->thread); CloseHandle(reactor->port); BeskidNetworkFree(reactor, sizeof(*reactor));
}
int32_t beskid_rt_v5_intrinsic_network_bind(void *value, void *wire, int32_t backlog) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_CLOSED;
  struct sockaddr_storage address; BeskidSocklen length;
  int status = BeskidNetworkDecode(wire, &address, &length); if (status) return status;
  if (bind(socket->socket, (struct sockaddr *)&address, length)) return BeskidNetworkError(WSAGetLastError());
  if (socket->kind == 1 && listen(socket->socket, backlog > 0 ? backlog : SOMAXCONN)) return BeskidNetworkError(WSAGetLastError());
  return NET_OK;
}
int32_t beskid_rt_v5_intrinsic_network_address(void *value, uint8_t peer, void *wire) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_CLOSED;
  struct sockaddr_storage address; int length = sizeof(address);
  int result = peer ? getpeername(socket->socket, (struct sockaddr *)&address, &length) : getsockname(socket->socket, (struct sockaddr *)&address, &length);
  return result ? BeskidNetworkError(WSAGetLastError()) : BeskidNetworkEncode((struct sockaddr *)&address, wire);
}
int32_t beskid_rt_v5_intrinsic_network_options(void *value, size_t bits) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_CLOSED;
  if (bits & ~(size_t)3 || (socket->kind != 1 && bits)) return NET_UNSUPPORTED;
  if (socket->kind != 1) return NET_OK;
  int no_delay = (bits & 1) != 0, keep_alive = (bits & 2) != 0;
  if (setsockopt(socket->socket, IPPROTO_TCP, TCP_NODELAY, (const char *)&no_delay, sizeof(no_delay)) || setsockopt(socket->socket, SOL_SOCKET, SO_KEEPALIVE, (const char *)&keep_alive, sizeof(keep_alive))) return BeskidNetworkError(WSAGetLastError());
  return NET_OK;
}
int64_t beskid_rt_v5_intrinsic_network_get_options(void *value) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return -NET_CLOSED;
  if (socket->kind != 1) return 0;
  int no_delay = 0, keep_alive = 0, length = sizeof(int);
  if (getsockopt(socket->socket, IPPROTO_TCP, TCP_NODELAY, (char *)&no_delay, &length) || getsockopt(socket->socket, SOL_SOCKET, SO_KEEPALIVE, (char *)&keep_alive, &length)) return -BeskidNetworkError(WSAGetLastError());
  return (no_delay ? 1 : 0) | (keep_alive ? 2 : 0);
}
int32_t beskid_rt_v5_intrinsic_network_shutdown_write(void *value) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_CLOSED;
  return shutdown(socket->socket, SD_SEND) ? BeskidNetworkError(WSAGetLastError()) : NET_OK;
}
