/* getaddrinfo failure classification for Runtime.Network name resolution.
 * No runtime dependency: tests/fixtures/network_dns.c checks it on every native host.
 * Windows callers include <winsock2.h> and <ws2tcpip.h> first. */
#ifndef BESKID_NATIVE_NETWORK_DNS_H
#define BESKID_NATIVE_NETWORK_DNS_H
#ifndef _WIN32
#include <netdb.h>
#endif

/* A name with no usable address is HostNotFound, whatever the resolver calls it:
 * EAI_NONAME (unknown name), EAI_NODATA (a known name without address records) and
 * EAI_ADDRFAMILY (no address of the requested family). glibc and musl declare the last
 * two only under _GNU_SOURCE, and the Linux runtime builds with _POSIX_C_SOURCE alone,
 * so their fixed Linux values stand in. macOS declares both under _DARWIN_C_SOURCE.
 * Windows defines EAI_NODATA as EAI_NONAME and can also return WSANO_DATA directly. */
#if defined(EAI_NODATA)
#define BESKID_EAI_NODATA EAI_NODATA
#elif defined(__linux__)
#define BESKID_EAI_NODATA (-5)
#endif
#if defined(EAI_ADDRFAMILY)
#define BESKID_EAI_ADDRFAMILY EAI_ADDRFAMILY
#elif defined(__linux__)
#define BESKID_EAI_ADDRFAMILY (-9)
#endif

enum BeskidNetworkDnsFailure {
  BESKID_DNS_HOST_NOT_FOUND,
  BESKID_DNS_UNSUPPORTED,
  BESKID_DNS_RESOURCE_EXHAUSTED,
  BESKID_DNS_NETWORK_DOWN,
};

static enum BeskidNetworkDnsFailure BeskidNetworkDnsClassify(int error) {
  if (error == EAI_NONAME) return BESKID_DNS_HOST_NOT_FOUND;
#ifdef BESKID_EAI_NODATA
  if (error == BESKID_EAI_NODATA) return BESKID_DNS_HOST_NOT_FOUND;
#endif
#ifdef BESKID_EAI_ADDRFAMILY
  if (error == BESKID_EAI_ADDRFAMILY) return BESKID_DNS_HOST_NOT_FOUND;
#endif
#ifdef _WIN32
  if (error == WSANO_DATA) return BESKID_DNS_HOST_NOT_FOUND;
#endif
  if (error == EAI_FAMILY) return BESKID_DNS_UNSUPPORTED;
  if (error == EAI_MEMORY) return BESKID_DNS_RESOURCE_EXHAUSTED;
  /* EAI_AGAIN, EAI_FAIL, EAI_SYSTEM and the rest: the resolver itself is unreachable. */
  return BESKID_DNS_NETWORK_DOWN;
}
#endif
