#include "beskid_runtime_abi_v5.h"
#include <stdalign.h>
static alignas(BESKID_RUNTIME_STATE_ALIGNMENT) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE];
void *glue_fixture_initialize(void) { return beskid_rt_v5_process_init(runtime); }
void glue_fixture_shutdown(void) { beskid_rt_v5_process_shutdown(runtime); }
