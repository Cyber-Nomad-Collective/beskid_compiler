#define _GNU_SOURCE
#include <dlfcn.h>
#include <netdb.h>
#include <pthread.h>
#include <time.h>
#include <string.h>
#include <unistd.h>

typedef int (*getaddrinfo_fn)(const char *, const char *, const struct addrinfo *, struct addrinfo **);

static pthread_mutex_t gate_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t gate_cond = PTHREAD_COND_INITIALIZER;
static int resolver_entered;
static int resolver_released;
static int resolver_completed;

static int wait_until(int *condition) {
    struct timespec deadline;
    if (clock_gettime(CLOCK_REALTIME, &deadline) != 0) return -1;
    deadline.tv_sec += 5;
    while (!*condition) {
        int status = pthread_cond_timedwait(&gate_cond, &gate_mutex, &deadline);
        if (status != 0) return -1;
    }
    return 0;
}

static int call_real_getaddrinfo(const char *node, const char *service, const struct addrinfo *hints, struct addrinfo **result) {
    getaddrinfo_fn real_getaddrinfo = (getaddrinfo_fn)dlsym(RTLD_NEXT, "getaddrinfo");
    if (!real_getaddrinfo) return EAI_SYSTEM;
    return real_getaddrinfo(node, service, hints, result);
}

static int resolve_loopback(const char *service, const struct addrinfo *hints, struct addrinfo **result) {
    return call_real_getaddrinfo("127.0.0.1", service, hints, result);
}

int getaddrinfo(const char *node, const char *service, const struct addrinfo *hints, struct addrinfo **result) {
    if (!node) return call_real_getaddrinfo(node, service, hints, result);

    if (strcmp(node, "beskid-deadline-block.invalid") == 0) {
        pthread_mutex_lock(&gate_mutex);
        resolver_entered = 1;
        static const char entered[] = "BESKID_DNS_DEADLINE_SHIM:blocked-entered\n";
        (void)write(STDERR_FILENO, entered, sizeof entered - 1);
        pthread_cond_broadcast(&gate_cond);
        int released = wait_until(&resolver_released);
        pthread_mutex_unlock(&gate_mutex);
        int status = released == 0 ? resolve_loopback(service, hints, result) : EAI_AGAIN;
        pthread_mutex_lock(&gate_mutex);
        resolver_completed = 1;
        pthread_cond_broadcast(&gate_cond);
        pthread_mutex_unlock(&gate_mutex);
        return status;
    }

    if (strcmp(node, "beskid-deadline-observe.invalid") == 0) {
        pthread_mutex_lock(&gate_mutex);
        int entered = wait_until(&resolver_entered);
        pthread_mutex_unlock(&gate_mutex);
        if (entered == 0) {
            static const char observed[] = "BESKID_DNS_DEADLINE_SHIM:observer-saw-blocked\n";
            (void)write(STDERR_FILENO, observed, sizeof observed - 1);
        }
        return entered == 0 ? resolve_loopback(service, hints, result) : EAI_AGAIN;
    }

    if (strcmp(node, "beskid-deadline-release.invalid") == 0) {
        pthread_mutex_lock(&gate_mutex);
        int entered = wait_until(&resolver_entered);
        if (entered == 0) {
            resolver_released = 1;
            pthread_cond_broadcast(&gate_cond);
            entered = wait_until(&resolver_completed);
        }
        pthread_mutex_unlock(&gate_mutex);
        if (entered == 0) {
            static const char released[] = "BESKID_DNS_DEADLINE_SHIM:release-saw-completion\n";
            (void)write(STDERR_FILENO, released, sizeof released - 1);
        }
        return entered == 0 ? resolve_loopback(service, hints, result) : EAI_AGAIN;
    }

    return call_real_getaddrinfo(node, service, hints, result);
}
