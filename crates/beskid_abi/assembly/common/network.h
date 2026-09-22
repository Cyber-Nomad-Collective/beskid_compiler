/* Narrow native transport. Runtime.Network owns handles and terminal winners.
 * Address wire: family byte (4/6), big-endian port, sixteen address bytes.
 * Native objects never cross the corelib service boundary. */
#ifndef BESKID_NATIVE_NETWORK_H
#define BESKID_NATIVE_NETWORK_H
#include <string.h>
static int BeskidNetworkPlatformReady(void);
#ifdef _WIN32
typedef SOCKET BeskidSocket;
typedef int BeskidSocklen;
#define BESKID_INVALID_SOCKET INVALID_SOCKET
#define BeskidSocketError() WSAGetLastError()
#define BeskidSocketClose(s) closesocket(s)
#else
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <sys/socket.h>
typedef int BeskidSocket;
typedef socklen_t BeskidSocklen;
#define BESKID_INVALID_SOCKET (-1)
#define BeskidSocketError() errno
#define BeskidSocketClose(s) close(s)
#endif

enum { NET_OK, NET_ADDRESS_IN_USE, NET_ADDRESS_NOT_AVAILABLE,
  NET_CONNECTION_ABORTED, NET_CONNECTION_REFUSED, NET_CONNECTION_RESET,
  NET_HOST_NOT_FOUND, NET_INVALID_ADDRESS, NET_MESSAGE_TOO_LARGE,
  NET_NETWORK_DOWN, NET_NOT_CONNECTED, NET_PERMISSION_DENIED, NET_TIMED_OUT,
  NET_CANCELLED, NET_CLOSED, NET_BUSY, NET_CLEANUP_FAILED, NET_UNSUPPORTED,
  NET_RESOURCE_EXHAUSTED, NET_PENDING };
/* Positional mirror of manifest status "BeskidNetworkStatus"; beskid_abi tests check parity. */
_Static_assert(NET_RESOURCE_EXHAUSTED == 18 && NET_PENDING == 19, "network status ABI");
enum { NET_CONNECT = 1, NET_ACCEPT, NET_READ, NET_WRITE, NET_RECEIVE, NET_SEND };
struct BeskidNetworkRequest {
  uintptr_t token, owner, operation;
  void *buffer;
  size_t length;
  uint8_t *address;
  size_t count;
  int32_t status;
  void *accepted, *native_state;
  size_t truncated;
};
_Static_assert(sizeof(struct BeskidNetworkRequest) == BESKID_NETWORK_REQUEST_SIZE, "network request ABI");
_Static_assert(offsetof(struct BeskidNetworkRequest, native_state) == BESKID_NETWORK_REQUEST_NATIVE_STATE_OFFSET, "network native state ABI");
_Static_assert(offsetof(struct BeskidNetworkRequest, status) == BESKID_NETWORK_REQUEST_STATUS_OFFSET, "network status ABI");

static void *BeskidNetworkAllocate(size_t size) {
  void *result = beskid_rt_v5_intrinsic_system_allocate(size ? size : 1, 8);
  if (result) memset(result, 0, size ? size : 1);
  return result;
}
static void BeskidNetworkFree(void *value, size_t size) {
  if (value) beskid_rt_v5_intrinsic_system_free(value, size ? size : 1);
}
static int32_t BeskidNetworkError(int error) {
#ifdef _WIN32
#define NET_ERR(name) WSA##name
#else
#define NET_ERR(name) name
#endif
  switch (error) {
    case 0: return NET_OK;
    case NET_ERR(EADDRINUSE): return NET_ADDRESS_IN_USE;
    case NET_ERR(EADDRNOTAVAIL): return NET_ADDRESS_NOT_AVAILABLE;
    case NET_ERR(ECONNABORTED): return NET_CONNECTION_ABORTED;
    case NET_ERR(ECONNREFUSED): return NET_CONNECTION_REFUSED;
    case NET_ERR(ECONNRESET): return NET_CONNECTION_RESET;
    case NET_ERR(EINVAL): case NET_ERR(EAFNOSUPPORT): return NET_INVALID_ADDRESS;
    case NET_ERR(EMSGSIZE): return NET_MESSAGE_TOO_LARGE;
    case NET_ERR(ENETDOWN): case NET_ERR(ENETUNREACH): case NET_ERR(EHOSTUNREACH): return NET_NETWORK_DOWN;
    case NET_ERR(ENOTCONN): return NET_NOT_CONNECTED;
    case NET_ERR(EACCES): return NET_PERMISSION_DENIED;
    case NET_ERR(ETIMEDOUT): return NET_TIMED_OUT;
    case NET_ERR(ENOTSOCK): case NET_ERR(EBADF): return NET_CLOSED;
    case NET_ERR(EWOULDBLOCK): case NET_ERR(EINPROGRESS): case NET_ERR(EALREADY): return NET_PENDING;
    case NET_ERR(EOPNOTSUPP): case NET_ERR(EPROTONOSUPPORT): return NET_UNSUPPORTED;
    case NET_ERR(EMFILE): case NET_ERR(ENOBUFS): return NET_RESOURCE_EXHAUSTED;
#ifdef _WIN32
    case WSA_OPERATION_ABORTED: return NET_CANCELLED;
    case WSA_NOT_ENOUGH_MEMORY: return NET_RESOURCE_EXHAUSTED;
#else
    case ENFILE: case ENOMEM: return NET_RESOURCE_EXHAUSTED;
    case ECANCELED: return NET_CANCELLED;
    case EINTR: return NET_PENDING;
    case EPIPE: return NET_CONNECTION_RESET;
    case EPERM: return NET_PERMISSION_DENIED;
#endif
    default: return NET_NETWORK_DOWN;
  }
#undef NET_ERR
}
static int BeskidNetworkDecode(const uint8_t *wire, struct sockaddr_storage *address, BeskidSocklen *length) {
  if (!wire) return NET_INVALID_ADDRESS;
  memset(address, 0, sizeof(*address));
  if (wire[0] == 4) {
    struct sockaddr_in *v4 = (struct sockaddr_in *)address;
    v4->sin_family = AF_INET;
    memcpy(&v4->sin_port, wire + 1, 2);
    memcpy(&v4->sin_addr, wire + 3, 4);
    *length = sizeof(*v4);
  } else if (wire[0] == 6) {
    struct sockaddr_in6 *v6 = (struct sockaddr_in6 *)address;
    v6->sin6_family = AF_INET6;
    memcpy(&v6->sin6_port, wire + 1, 2);
    memcpy(&v6->sin6_addr, wire + 3, 16);
    *length = sizeof(*v6);
  } else return NET_INVALID_ADDRESS;
  return NET_OK;
}
static int BeskidNetworkEncode(const struct sockaddr *address, uint8_t *wire) {
  if (!wire) return NET_INVALID_ADDRESS;
  memset(wire, 0, 19);
  if (address->sa_family == AF_INET) {
    const struct sockaddr_in *v4 = (const struct sockaddr_in *)address;
    wire[0] = 4; memcpy(wire + 1, &v4->sin_port, 2); memcpy(wire + 3, &v4->sin_addr, 4);
  } else if (address->sa_family == AF_INET6) {
    const struct sockaddr_in6 *v6 = (const struct sockaddr_in6 *)address;
    wire[0] = 6; memcpy(wire + 1, &v6->sin6_port, 2); memcpy(wire + 3, &v6->sin6_addr, 16);
  } else return NET_UNSUPPORTED;
  return NET_OK;
}

/* Resolver work uses the existing bounded Foundation pool. Native result memory
 * remains owned by that job until release, including cancelled caller waits. */
struct BeskidNetworkDns { uint8_t *addresses; size_t count; uintptr_t port; int32_t family; char host[]; };
static void BeskidNetworkDnsPerform(struct BeskidWorkerRequest *request) {
  struct BeskidNetworkDns *job = (void *)request->buffer;
  struct addrinfo hints, *results = NULL;
  memset(&hints, 0, sizeof(hints));
  hints.ai_family = job->family == 4 ? AF_INET : job->family == 6 ? AF_INET6 : AF_UNSPEC;
  hints.ai_socktype = SOCK_STREAM;
  int error = getaddrinfo(job->host, NULL, &hints, &results);
  if (error) {
    request->error = error == EAI_NONAME ? NET_HOST_NOT_FOUND : error == EAI_FAMILY ? NET_UNSUPPORTED
      : error == EAI_MEMORY ? NET_RESOURCE_EXHAUSTED : NET_NETWORK_DOWN;
    request->result = -1; return;
  }
  size_t count = 0;
  for (struct addrinfo *entry = results; entry; entry = entry->ai_next)
    if (entry->ai_family == AF_INET || entry->ai_family == AF_INET6) ++count;
  if (count > SIZE_MAX / 19) { freeaddrinfo(results); request->error = NET_RESOURCE_EXHAUSTED; request->result = -1; return; }
  job->addresses = BeskidNetworkAllocate(count * 19);
  if (!job->addresses) { freeaddrinfo(results); request->error = NET_RESOURCE_EXHAUSTED; request->result = -1; return; }
  for (struct addrinfo *entry = results; entry; entry = entry->ai_next) {
    if (entry->ai_family != AF_INET && entry->ai_family != AF_INET6) continue;
    uint8_t *wire = job->addresses + job->count++ * 19;
    BeskidNetworkEncode(entry->ai_addr, wire);
    wire[1] = (uint8_t)(job->port >> 8); wire[2] = (uint8_t)job->port;
  }
  freeaddrinfo(results);
  request->result = (intptr_t)job->count;
  request->error = job->count ? NET_OK : NET_HOST_NOT_FOUND;
}
static void BeskidNetworkDnsFree(struct BeskidWorkerRequest *request) {
  struct BeskidNetworkDns *job = (void *)request->buffer;
  BeskidNetworkFree(job->addresses, job->count * 19);
}
void *beskid_rt_v5_intrinsic_network_dns_start(void *host_view, size_t port, int32_t family, uintptr_t owner, uintptr_t token) {
  struct NetworkStringView { const uint8_t *data; size_t length; };
  const struct NetworkStringView *host = host_view;
  if (BeskidNetworkPlatformReady()) return NULL;
  if (!host || !host->data || !host->length || port > 65535 || (family != 0 && family != 4 && family != 6) || host->length > SIZE_MAX - sizeof(struct BeskidNetworkDns) - 1) return NULL;
  for (size_t i = 0; i < host->length; ++i) if (!host->data[i]) return NULL;
  size_t bytes = sizeof(struct BeskidNetworkDns) + host->length + 1;
  struct BeskidWorkerRequest *request = BeskidNetworkAllocate(sizeof(*request));
  struct BeskidNetworkDns *job = BeskidNetworkAllocate(bytes);
  if (!request || !job) { BeskidNetworkFree(request, sizeof(*request)); BeskidNetworkFree(job, bytes); return NULL; }
  job->port = port; job->family = family; memcpy(job->host, host->data, host->length);
  request->operation = 3; request->buffer = (void *)job; request->length = bytes;
  request->owner = owner; request->tag = token;
  if (beskid_rt_v5_intrinsic_worker_submit(request)) { BeskidWorkerFree(request); return NULL; }
  return request;
}
int64_t beskid_rt_v5_intrinsic_network_dns_count(void *value) {
  struct BeskidWorkerRequest *request = value;
  if (!request) return -NET_CLOSED;
  BeskidLock();
  int64_t result = request->state != BESKID_WORKER_COMPLETE ? -NET_PENDING : request->error ? -request->error : request->result;
  BeskidUnlock(); return result;
}
int32_t beskid_rt_v5_intrinsic_network_dns_get(void *value, size_t index, void *address) {
  struct BeskidWorkerRequest *request = value;
  int64_t count = beskid_rt_v5_intrinsic_network_dns_count(value);
  if (count < 0) return (int32_t)-count;
  if (index >= (size_t)count || !address) return NET_INVALID_ADDRESS;
  struct BeskidNetworkDns *job = (void *)request->buffer;
  memcpy(address, job->addresses + index * 19, 19); return NET_OK;
}
void beskid_rt_v5_intrinsic_network_dns_release(void *request) { beskid_rt_v5_intrinsic_worker_release(request); }
static size_t BeskidNetworkDiagnosticWord(char *output, uintptr_t value) {
  char digits[24]; size_t count = 0;
  do { digits[count++] = (char)('0' + value % 10); value /= 10; } while (value);
  for (size_t i = 0; i < count; ++i) output[i] = digits[count - i - 1];
  return count;
}
void beskid_rt_v5_intrinsic_network_report_leak(uintptr_t slot, uintptr_t generation, uintptr_t owner, uintptr_t kind) {
#ifdef _WIN32
  const char prefix[] = "beskid network leak backend=iocp slot=";
#elif defined(__APPLE__)
  const char prefix[] = "beskid network leak backend=kqueue slot=";
#else
  const char prefix[] = "beskid network leak backend=epoll slot=";
#endif
  char text[256]; size_t count = sizeof(prefix) - 1;
  memcpy(text, prefix, count);
  count += BeskidNetworkDiagnosticWord(text + count, slot);
  memcpy(text + count, " generation=", 12); count += 12;
  count += BeskidNetworkDiagnosticWord(text + count, generation);
  memcpy(text + count, " owner=", 7); count += 7;
  count += BeskidNetworkDiagnosticWord(text + count, owner);
  memcpy(text + count, " resource_kind=", 15); count += 15;
  count += BeskidNetworkDiagnosticWord(text + count, kind);
  const char suffix[] = " operation=shutdown winner=live leak_count=1\n";
  memcpy(text + count, suffix, sizeof(suffix) - 1); count += sizeof(suffix) - 1;
#ifdef _WIN32
  DWORD written; WriteFile(GetStdHandle(STD_ERROR_HANDLE), text, (DWORD)count, &written, NULL);
#else
  size_t sent = 0;
  while (sent < count) { ssize_t written = write(2, text + sent, count - sent); if (written < 0 && errno == EINTR) continue; if (written <= 0) break; sent += (size_t)written; }
#endif
}
#endif
