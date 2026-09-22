#include "network.h"
#include <sys/uio.h>
static int BeskidNetworkPlatformReady(void) { return NET_OK; }
#ifdef __APPLE__
#include <sys/event.h>
#else
#include <sys/epoll.h>
#endif

struct BeskidNetworkReactor;
struct BeskidNativeSocket {
  struct BeskidNativeSocket *next;
  struct BeskidNetworkReactor *reactor;
  uint64_t identity;
  BeskidSocket socket;
  int family, kind, registered;
  struct BeskidNetworkRequest *read, *write;
};
struct BeskidNetworkReactor {
  int queue, wake[2], stopping;
  pthread_t thread;
  struct BeskidNativeSocket *sockets;
};
static pthread_mutex_t beskid_network_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t beskid_network_table_lock = PTHREAD_MUTEX_INITIALIZER;
static uintptr_t beskid_network_table;
static uint64_t beskid_network_identity;
void beskid_rt_v5_intrinsic_network_table_lock(void) { pthread_mutex_lock(&beskid_network_table_lock); }
void beskid_rt_v5_intrinsic_network_table_unlock(void) { pthread_mutex_unlock(&beskid_network_table_lock); }
void *beskid_rt_v5_intrinsic_network_table_root(void) { return &beskid_network_table; }
static int BeskidNetworkSocketConfigure(int socket) {
  int flags = fcntl(socket, F_GETFL, 0);
  if (flags < 0 || fcntl(socket, F_SETFL, flags | O_NONBLOCK) < 0 || fcntl(socket, F_SETFD, FD_CLOEXEC) < 0) return BeskidNetworkError(errno);
#ifdef __APPLE__
  int yes = 1;
  if (setsockopt(socket, SOL_SOCKET, SO_NOSIGPIPE, &yes, sizeof(yes))) return BeskidNetworkError(errno);
#endif
  return NET_OK;
}
static struct BeskidNativeSocket *BeskidNetworkWrap(int socket, int family, int kind) {
  struct BeskidNativeSocket *value = BeskidNetworkAllocate(sizeof(*value));
  if (!value) { close(socket); return NULL; }
  value->socket = socket; value->family = family; value->kind = kind;
  return value;
}
int32_t beskid_rt_v5_intrinsic_network_open(int32_t kind, int32_t family, void *output) {
  if (!output || (kind != 1 && kind != 2) || (family != 4 && family != 6)) return NET_INVALID_ADDRESS;
  *(void **)output = NULL;
  int descriptor = socket(family == 4 ? AF_INET : AF_INET6, kind == 1 ? SOCK_STREAM : SOCK_DGRAM, 0);
  if (descriptor < 0) return BeskidNetworkError(errno);
  int status = BeskidNetworkSocketConfigure(descriptor);
  if (status) { close(descriptor); return status; }
  struct BeskidNativeSocket *value = BeskidNetworkWrap(descriptor, family, kind);
  if (!value) return NET_RESOURCE_EXHAUSTED;
  *(void **)output = value; return NET_OK;
}
static int BeskidNetworkInterest(struct BeskidNativeSocket *socket) {
  struct BeskidNetworkReactor *reactor = socket->reactor;
  if (!reactor) return NET_OK;
#ifdef __APPLE__
  struct kevent changes[2];
  int count = 0;
  if (socket->read || (socket->registered & 1)) {
    EV_SET(&changes[count++], socket->socket, EVFILT_READ, socket->read ? EV_ADD | EV_ENABLE : EV_DELETE, 0, 0, (void *)(uintptr_t)socket->identity);
  }
  if (socket->write || (socket->registered & 2)) {
    EV_SET(&changes[count++], socket->socket, EVFILT_WRITE, socket->write ? EV_ADD | EV_ENABLE : EV_DELETE, 0, 0, (void *)(uintptr_t)socket->identity);
  }
  if (count && kevent(reactor->queue, changes, count, NULL, 0, NULL) < 0) return BeskidNetworkError(errno);
  socket->registered = (socket->read ? 1 : 0) | (socket->write ? 2 : 0);
#else
  struct epoll_event event;
  memset(&event, 0, sizeof(event));
  event.events = (socket->read ? EPOLLIN : 0) | (socket->write ? EPOLLOUT : 0) | EPOLLRDHUP;
  event.data.u64 = socket->identity;
  if (socket->read || socket->write) {
    if (epoll_ctl(reactor->queue, socket->registered ? EPOLL_CTL_MOD : EPOLL_CTL_ADD, socket->socket, &event) < 0) return BeskidNetworkError(errno);
    socket->registered = 1;
  } else if (socket->registered) {
    if (epoll_ctl(reactor->queue, EPOLL_CTL_DEL, socket->socket, NULL) < 0 && errno != ENOENT) return BeskidNetworkError(errno);
    socket->registered = 0;
  }
#endif
  return NET_OK;
}
static int BeskidNetworkAttempt(struct BeskidNativeSocket *socket, struct BeskidNetworkRequest *request, int initial) {
  struct sockaddr_storage address;
  BeskidSocklen length = sizeof(address);
  ssize_t count = -1;
  int flags = 0;
#ifndef __APPLE__
  flags = MSG_NOSIGNAL;
#endif
  switch (request->operation) {
    case NET_CONNECT: {
      int status = BeskidNetworkDecode(request->address, &address, &length);
      if (status) return status;
      if (initial) {
        if (!connect(socket->socket, (struct sockaddr *)&address, length)) return NET_OK;
        return BeskidNetworkError(errno);
      }
      int error = 0; length = sizeof(error);
      if (getsockopt(socket->socket, SOL_SOCKET, SO_ERROR, &error, &length)) return BeskidNetworkError(errno);
      return BeskidNetworkError(error);
    }
    case NET_ACCEPT: {
      int accepted = accept(socket->socket, (struct sockaddr *)&address, &length);
      if (accepted < 0) return BeskidNetworkError(errno);
      int status = BeskidNetworkSocketConfigure(accepted);
      if (status) { close(accepted); return status; }
      request->accepted = BeskidNetworkWrap(accepted, socket->family, 1);
      if (!request->accepted) return NET_RESOURCE_EXHAUSTED;
      if (request->address) BeskidNetworkEncode((struct sockaddr *)&address, request->address);
      return NET_OK;
    }
    case NET_READ: count = recv(socket->socket, request->buffer, request->length, 0); break;
    case NET_WRITE: count = send(socket->socket, request->buffer, request->length, flags); break;
    case NET_RECEIVE: {
      struct iovec vector = {request->buffer, request->length};
      struct msghdr message;
      memset(&message, 0, sizeof(message));
      message.msg_name = &address; message.msg_namelen = length;
      message.msg_iov = &vector; message.msg_iovlen = 1;
      count = recvmsg(socket->socket, &message, 0);
      if (count >= 0) {
        request->truncated = (message.msg_flags & MSG_TRUNC) != 0;
        if (request->address) BeskidNetworkEncode((struct sockaddr *)&address, request->address);
      }
      break;
    }
    case NET_SEND:
      if (request->address) {
        int status = BeskidNetworkDecode(request->address, &address, &length);
        if (status) return status;
        count = sendto(socket->socket, request->buffer, request->length, flags, (struct sockaddr *)&address, length);
      } else count = send(socket->socket, request->buffer, request->length, flags);
      break;
    default: return NET_UNSUPPORTED;
  }
  if (count < 0) return BeskidNetworkError(errno);
  request->count = (size_t)count; return NET_OK;
}
static void BeskidNetworkComplete(struct BeskidNetworkRequest **slot, int status) {
  struct BeskidNetworkRequest *request = *slot;
  if (!request) return;
  *slot = NULL; request->status = status; request->native_state = NULL;
  beskid_rt_v5_intrinsic_owner_post(request->owner, request->token, 1);
}
int32_t beskid_rt_v5_intrinsic_network_submit(void *reactor_value, void *socket_value, void *request_value) {
  struct BeskidNetworkReactor *reactor = reactor_value;
  struct BeskidNativeSocket *socket = socket_value;
  struct BeskidNetworkRequest *request = request_value;
  if (!reactor || !socket || !request) return NET_CLOSED;
  pthread_mutex_lock(&beskid_network_lock);
  int status = NET_CLOSED;
  if (reactor->stopping || (socket->reactor && socket->reactor != reactor)) goto done;
  if (!socket->reactor) {
    if (beskid_network_identity == UINT64_MAX) { status = NET_RESOURCE_EXHAUSTED; goto done; }
    socket->identity = ++beskid_network_identity;
    socket->reactor = reactor; socket->next = reactor->sockets; reactor->sockets = socket;
  }
  struct BeskidNetworkRequest **slot = (request->operation == NET_ACCEPT || request->operation == NET_READ || request->operation == NET_RECEIVE) ? &socket->read : &socket->write;
  if (*slot) { status = NET_BUSY; goto done; }
  request->count = 0; request->truncated = 0; request->accepted = NULL; request->native_state = NULL;
  status = BeskidNetworkAttempt(socket, request, 1);
  if (status == NET_PENDING) {
    *slot = request; request->native_state = socket;
    int registration = BeskidNetworkInterest(socket);
    if (registration) { *slot = NULL; request->native_state = NULL; status = registration; }
  }
done:
  request->status = status;
  pthread_mutex_unlock(&beskid_network_lock);
  return status;
}
void beskid_rt_v5_intrinsic_network_cancel(void *reactor_value, void *request_value) {
  (void)reactor_value;
  struct BeskidNetworkRequest *request = request_value;
  if (!request) return;
  pthread_mutex_lock(&beskid_network_lock);
  struct BeskidNativeSocket *socket = request->native_state;
  if (socket) {
    if (socket->read == request) socket->read = NULL;
    if (socket->write == request) socket->write = NULL;
    request->native_state = NULL; request->status = NET_CANCELLED;
    (void)BeskidNetworkInterest(socket);
  }
  pthread_mutex_unlock(&beskid_network_lock);
}
int32_t beskid_rt_v5_intrinsic_network_reactor_poll(void *value, int32_t timeout) {
  struct BeskidNetworkReactor *reactor = value;
  if (!reactor) return -NET_CLOSED;
#ifdef __APPLE__
  struct kevent events[64];
  struct timespec time = {timeout < 0 ? 0 : timeout / 1000, timeout < 0 ? 0 : (timeout % 1000) * 1000000};
  int count = kevent(reactor->queue, NULL, 0, events, 64, timeout < 0 ? NULL : &time);
#else
  struct epoll_event events[64];
  int count = epoll_wait(reactor->queue, events, 64, timeout);
#endif
  if (count < 0) return errno == EINTR ? 0 : -NET_NETWORK_DOWN;
  pthread_mutex_lock(&beskid_network_lock);
  for (int i = 0; i < count; ++i) {
#ifdef __APPLE__
    uint64_t identity = (uintptr_t)events[i].udata;
    int readable = events[i].filter == EVFILT_READ || (events[i].flags & (EV_ERROR | EV_EOF));
    int writable = events[i].filter == EVFILT_WRITE || (events[i].flags & (EV_ERROR | EV_EOF));
#else
    uint64_t identity = events[i].data.u64;
    int readable = events[i].events & (EPOLLIN | EPOLLERR | EPOLLHUP | EPOLLRDHUP);
    int writable = events[i].events & (EPOLLOUT | EPOLLERR | EPOLLHUP);
#endif
    if (!identity) { char bytes[64]; while (read(reactor->wake[0], bytes, sizeof(bytes)) > 0) {} continue; }
    struct BeskidNativeSocket *socket = reactor->sockets;
    while (socket && socket->identity != identity) socket = socket->next;
    if (!socket) continue; /* Removed identity: discard without dereferencing stale memory. */
    if (readable && socket->read) {
      int status = BeskidNetworkAttempt(socket, socket->read, 0);
      if (status != NET_PENDING) BeskidNetworkComplete(&socket->read, status);
    }
    if (writable && socket->write) {
      int status = BeskidNetworkAttempt(socket, socket->write, 0);
      if (status != NET_PENDING) BeskidNetworkComplete(&socket->write, status);
    }
    int status = BeskidNetworkInterest(socket);
    if (status) { BeskidNetworkComplete(&socket->read, status); BeskidNetworkComplete(&socket->write, status); }
  }
  pthread_mutex_unlock(&beskid_network_lock);
  return count;
}
static void *BeskidNetworkReactorMain(void *value) {
  struct BeskidNetworkReactor *reactor = value;
  for (;;) {
    pthread_mutex_lock(&beskid_network_lock);
    int stop = reactor->stopping;
    pthread_mutex_unlock(&beskid_network_lock);
    if (stop) return NULL;
    if (beskid_rt_v5_intrinsic_network_reactor_poll(reactor, -1) < 0) {
      pthread_mutex_lock(&beskid_network_lock);
      reactor->stopping = 1;
      for (struct BeskidNativeSocket *socket = reactor->sockets; socket; socket = socket->next) {
        BeskidNetworkComplete(&socket->read, NET_NETWORK_DOWN); BeskidNetworkComplete(&socket->write, NET_NETWORK_DOWN);
      }
      pthread_mutex_unlock(&beskid_network_lock); return NULL;
    }
  }
}
void *beskid_rt_v5_intrinsic_network_reactor_create(void) {
  struct BeskidNetworkReactor *reactor = BeskidNetworkAllocate(sizeof(*reactor));
  if (!reactor) return NULL;
#ifdef __APPLE__
  reactor->queue = kqueue();
#else
  reactor->queue = epoll_create1(EPOLL_CLOEXEC);
#endif
  if (reactor->queue < 0) { BeskidNetworkFree(reactor, sizeof(*reactor)); return NULL; }
  if (pipe(reactor->wake)) { close(reactor->queue); BeskidNetworkFree(reactor, sizeof(*reactor)); return NULL; }
  if (fcntl(reactor->queue, F_SETFD, FD_CLOEXEC) < 0 ||
      fcntl(reactor->wake[0], F_SETFL, O_NONBLOCK) < 0 || fcntl(reactor->wake[1], F_SETFL, O_NONBLOCK) < 0 ||
      fcntl(reactor->wake[0], F_SETFD, FD_CLOEXEC) < 0 || fcntl(reactor->wake[1], F_SETFD, FD_CLOEXEC) < 0) goto fail;
#ifdef __APPLE__
  struct kevent event;
  EV_SET(&event, reactor->wake[0], EVFILT_READ, EV_ADD, 0, 0, NULL);
  if (kevent(reactor->queue, &event, 1, NULL, 0, NULL) < 0) goto fail;
#else
  struct epoll_event event;
  memset(&event, 0, sizeof(event)); event.events = EPOLLIN;
  if (epoll_ctl(reactor->queue, EPOLL_CTL_ADD, reactor->wake[0], &event) < 0) goto fail;
#endif
  if (pthread_create(&reactor->thread, NULL, BeskidNetworkReactorMain, reactor)) goto fail;
  return reactor;
fail:
  close(reactor->wake[0]); close(reactor->wake[1]); close(reactor->queue);
  BeskidNetworkFree(reactor, sizeof(*reactor)); return NULL;
}
int32_t beskid_rt_v5_intrinsic_network_close(void *value) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_OK;
  pthread_mutex_lock(&beskid_network_lock);
  if (socket->reactor) {
    BeskidNetworkComplete(&socket->read, NET_CLOSED); BeskidNetworkComplete(&socket->write, NET_CLOSED);
    (void)BeskidNetworkInterest(socket);
    struct BeskidNativeSocket **link = &socket->reactor->sockets;
    while (*link && *link != socket) link = &(*link)->next;
    if (*link) *link = socket->next;
  }
  /* close is not retried on EINTR: the descriptor may already have been reused. */
  int result = close(socket->socket);
  int status = result ? NET_CLEANUP_FAILED : NET_OK;
  BeskidNetworkFree(socket, sizeof(*socket));
  pthread_mutex_unlock(&beskid_network_lock); return status;
}
void beskid_rt_v5_intrinsic_network_reactor_destroy(void *value) {
  struct BeskidNetworkReactor *reactor = value;
  if (!reactor) return;
  pthread_mutex_lock(&beskid_network_lock); reactor->stopping = 1; pthread_mutex_unlock(&beskid_network_lock);
  char wake = 1; (void)write(reactor->wake[1], &wake, 1);
  pthread_join(reactor->thread, NULL);
  while (reactor->sockets) beskid_rt_v5_intrinsic_network_close(reactor->sockets);
  close(reactor->wake[0]); close(reactor->wake[1]); close(reactor->queue); BeskidNetworkFree(reactor, sizeof(*reactor));
}
int32_t beskid_rt_v5_intrinsic_network_bind(void *value, void *wire, int32_t backlog) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_CLOSED;
  struct sockaddr_storage address; BeskidSocklen length;
  int status = BeskidNetworkDecode(wire, &address, &length);
  if (status) return status;
  if (bind(socket->socket, (struct sockaddr *)&address, length)) return BeskidNetworkError(errno);
  if (socket->kind == 1 && listen(socket->socket, backlog > 0 ? backlog : SOMAXCONN)) return BeskidNetworkError(errno);
  return NET_OK;
}
int32_t beskid_rt_v5_intrinsic_network_address(void *value, uint8_t peer, void *wire) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_CLOSED;
  struct sockaddr_storage address; BeskidSocklen length = sizeof(address);
  int result = peer ? getpeername(socket->socket, (struct sockaddr *)&address, &length) : getsockname(socket->socket, (struct sockaddr *)&address, &length);
  return result ? BeskidNetworkError(errno) : BeskidNetworkEncode((struct sockaddr *)&address, wire);
}
int32_t beskid_rt_v5_intrinsic_network_options(void *value, size_t bits) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_CLOSED;
  if (bits & ~(size_t)3 || (socket->kind != 1 && bits)) return NET_UNSUPPORTED;
  if (socket->kind != 1) return NET_OK;
  int no_delay = (bits & 1) != 0, keep_alive = (bits & 2) != 0;
  if (setsockopt(socket->socket, IPPROTO_TCP, TCP_NODELAY, &no_delay, sizeof(no_delay)) || setsockopt(socket->socket, SOL_SOCKET, SO_KEEPALIVE, &keep_alive, sizeof(keep_alive))) return BeskidNetworkError(errno);
  return NET_OK;
}
int64_t beskid_rt_v5_intrinsic_network_get_options(void *value) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return -NET_CLOSED;
  if (socket->kind != 1) return 0;
  int no_delay = 0, keep_alive = 0; BeskidSocklen length = sizeof(int);
  if (getsockopt(socket->socket, IPPROTO_TCP, TCP_NODELAY, &no_delay, &length) || getsockopt(socket->socket, SOL_SOCKET, SO_KEEPALIVE, &keep_alive, &length)) return -BeskidNetworkError(errno);
  return (no_delay ? 1 : 0) | (keep_alive ? 2 : 0);
}
int32_t beskid_rt_v5_intrinsic_network_shutdown_write(void *value) {
  struct BeskidNativeSocket *socket = value;
  if (!socket) return NET_CLOSED;
  return shutdown(socket->socket, SHUT_WR) ? BeskidNetworkError(errno) : NET_OK;
}
