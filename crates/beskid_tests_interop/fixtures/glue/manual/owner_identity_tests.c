/* Tests ONLY the native canonical issuer. No foreign thread enters Beskid GC. */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#endif
extern uint64_t beskid_glue_v1_next_identity(void);
#define WORKERS 16
#define PER_WORKER 1024
static uint64_t values[WORKERS * PER_WORKER];
#ifdef _WIN32
static DWORD WINAPI issue(LPVOID arg)
#else
static void *issue(void *arg)
#endif
{
    size_t index = (size_t)(uintptr_t)arg;
    for (size_t i = 0; i < PER_WORKER; ++i) {
        values[index * PER_WORKER + i] = beskid_glue_v1_next_identity();
        assert(values[index * PER_WORKER + i] != 0);
        if (i) assert(values[index * PER_WORKER + i] > values[index * PER_WORKER + i - 1]);
    }
    return 0;
}
static int compare(const void *a, const void *b) {
    uint64_t left = *(const uint64_t *)a, right = *(const uint64_t *)b;
    return (left > right) - (left < right);
}
int main(void) {
#ifdef _WIN32
    HANDLE workers[WORKERS];
    for (size_t i=0; i<WORKERS; ++i) { workers[i]=CreateThread(NULL,0,issue,(void *)(uintptr_t)i,0,NULL); assert(workers[i]); }
    assert(WaitForMultipleObjects(WORKERS,workers,TRUE,INFINITE)==WAIT_OBJECT_0);
    for (size_t i=0; i<WORKERS; ++i) assert(CloseHandle(workers[i]));
#else
    pthread_t workers[WORKERS];
    for (size_t i=0; i<WORKERS; ++i) assert(pthread_create(&workers[i],NULL,issue,(void *)(uintptr_t)i)==0);
    for (size_t i=0; i<WORKERS; ++i) assert(pthread_join(workers[i],NULL)==0);
#endif
    qsort(values,WORKERS*PER_WORKER,sizeof(uint64_t),compare);
    const uint64_t *flat=values;
    for (size_t i=1; i<WORKERS*PER_WORKER; ++i) assert(flat[i] > flat[i-1]);
    assert(beskid_glue_v1_next_identity() > flat[WORKERS*PER_WORKER-1]);
    puts("canonical issuer: 16384 unique nonzero concurrent identities");
    return 0;
}
