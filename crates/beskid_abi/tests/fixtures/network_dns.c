/* Host fixture for assembly/common/network_dns.h: getaddrinfo failure codes map to the
 * Runtime.Network status a caller sees. No lookup runs, so the result never depends on
 * the host resolver. Built like the runtime: _POSIX_C_SOURCE on Linux, _DARWIN_C_SOURCE
 * on macOS. */
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif
#define _POSIX_C_SOURCE 200809L
#ifdef _WIN32
#include <winsock2.h>
#include <ws2tcpip.h>
#endif
#include <stdio.h>
#include "../../assembly/common/network_dns.h"

static int failures;
#define CHECK(condition, label) do { if (!(condition)) { fprintf(stderr, "FAIL %s\n", label); ++failures; } } while (0)

int main(void) {
  CHECK(BeskidNetworkDnsClassify(EAI_NONAME) == BESKID_DNS_HOST_NOT_FOUND, "EAI_NONAME");
#ifdef BESKID_EAI_NODATA
  CHECK(BeskidNetworkDnsClassify(BESKID_EAI_NODATA) == BESKID_DNS_HOST_NOT_FOUND, "EAI_NODATA");
#elif defined(__linux__) || defined(__APPLE__)
  CHECK(0, "EAI_NODATA is classified on this host");
#endif
#ifdef BESKID_EAI_ADDRFAMILY
  CHECK(BeskidNetworkDnsClassify(BESKID_EAI_ADDRFAMILY) == BESKID_DNS_HOST_NOT_FOUND, "EAI_ADDRFAMILY");
#endif
#ifdef _WIN32
  CHECK(BeskidNetworkDnsClassify(WSANO_DATA) == BESKID_DNS_HOST_NOT_FOUND, "WSANO_DATA");
  CHECK(BeskidNetworkDnsClassify(WSAHOST_NOT_FOUND) == BESKID_DNS_HOST_NOT_FOUND, "WSAHOST_NOT_FOUND");
#endif
  CHECK(BeskidNetworkDnsClassify(EAI_FAMILY) == BESKID_DNS_UNSUPPORTED, "EAI_FAMILY");
  CHECK(BeskidNetworkDnsClassify(EAI_MEMORY) == BESKID_DNS_RESOURCE_EXHAUSTED, "EAI_MEMORY");
  CHECK(BeskidNetworkDnsClassify(EAI_AGAIN) == BESKID_DNS_NETWORK_DOWN, "EAI_AGAIN");
  CHECK(BeskidNetworkDnsClassify(EAI_FAIL) == BESKID_DNS_NETWORK_DOWN, "EAI_FAIL");
  if (failures) return 1;
  printf("dns=ok\n");
  return 0;
}
