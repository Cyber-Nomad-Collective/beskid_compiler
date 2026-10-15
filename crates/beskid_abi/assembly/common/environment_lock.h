/* One synchronization authority for canonical POSIX environment access. */
#ifndef BESKID_ENVIRONMENT_LOCK_H
#define BESKID_ENVIRONMENT_LOCK_H
#ifndef _WIN32
#include <pthread.h>
static pthread_mutex_t beskid_environment_lock = PTHREAD_MUTEX_INITIALIZER;
#define BESKID_ENVIRONMENT_LOCK() pthread_mutex_lock(&beskid_environment_lock)
#define BESKID_ENVIRONMENT_UNLOCK()                                            \
  pthread_mutex_unlock(&beskid_environment_lock)
#endif
#endif
