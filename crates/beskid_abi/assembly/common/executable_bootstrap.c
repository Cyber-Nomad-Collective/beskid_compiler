#include "../../include/beskid_runtime_abi_v5.h"
#include <stdlib.h>

/*
 * The executable host is the only owner of process-lifetime runtime activation.
 * It deliberately contains no Core.IO behaviour: Core.IO owns descriptor loops,
 * EOF, no-progress, and close semantics inside the Beskid runtime.
 */

/*
 * `BESKID_HEAP_MAX_BYTES` configures the growable managed heap's cap from the
 * host environment. Accepted syntax is a decimal integer with an optional
 * `K`/`M`/`G` suffix (powers of 1024). An unparsable value, or a value the
 * runtime rejects (below the already-committed size or below the initial
 * region size), is a host configuration error: the process traps with the
 * standard diagnostic line and exits 101 instead of running with a cap the
 * operator did not ask for. The runtime itself never reads the environment
 * for this setting; parsing lives here so the canonical `.bd` corpus keeps no
 * string literals and one activation owner keeps the decision.
 */
static int beskid_parse_heap_max_bytes(const char *text, uint64_t *out_bytes) {
  if (text == NULL || text[0] == '\0')
    return 0;
  uint64_t value = 0;
  const char *cursor = text;
  int saw_digit = 0;
  while (*cursor >= '0' && *cursor <= '9') {
    uint64_t next = value * 10u + (uint64_t)(*cursor - '0');
    if (next < value)
      return 0; /* overflow */
    value = next;
    saw_digit = 1;
    cursor++;
  }
  if (!saw_digit)
    return 0;
  uint64_t multiplier = 1;
  if (*cursor == 'K' || *cursor == 'k') {
    multiplier = 1024ULL;
    cursor++;
  } else if (*cursor == 'M' || *cursor == 'm') {
    multiplier = 1024ULL * 1024ULL;
    cursor++;
  } else if (*cursor == 'G' || *cursor == 'g') {
    multiplier = 1024ULL * 1024ULL * 1024ULL;
    cursor++;
  }
  if (*cursor != '\0')
    return 0;
  uint64_t scaled = value * multiplier;
  if (multiplier != 1 && value != 0 && scaled / multiplier != value)
    return 0; /* overflow */
  *out_bytes = scaled;
  return 1;
}

static void beskid_apply_heap_cap_from_environment(void) {
  const char *text = getenv("BESKID_HEAP_MAX_BYTES");
  if (text == NULL)
    return;
  static const char label[] = "BESKID_HEAP_MAX_BYTES";
  uint64_t bytes = 0;
  if (!beskid_parse_heap_max_bytes(text, &bytes) ||
      beskid_rt_v5_heap_set_cap((size_t)bytes) == 0) {
    beskid_rt_v5_trap(5, (void *)label, sizeof(label) - 1);
  }
}
#if defined(BESKID_EXECUTABLE_CORE_ARGS_UTF16)
#include <wchar.h>
_Static_assert(sizeof(wchar_t) == sizeof(uint16_t),
               "the Windows Core.Args bridge requires UTF-16 wchar_t");
extern void beskid_rt_v5_args_handoff_utf16(int64_t argc,
                                             const uint16_t *const *argv);
#elif defined(BESKID_EXECUTABLE_CORE_ARGS_UTF8)
extern void beskid_rt_v5_args_handoff_utf8(int64_t argc,
                                            const char *const *argv);
#endif

#if defined(BESKID_EXECUTABLE_PROGRAM_RETURNS_VOID)
extern void beskid_program_main(void);
#else
extern int64_t beskid_program_main(void);
#endif

static int beskid_execute_program(void) {
  _Alignas(BESKID_RUNTIME_STATE_ALIGNMENT)
      unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
  if (beskid_rt_v5_process_init(runtime) == NULL)
    return BESKID_TRAP_EXIT_STATUS;

#if defined(BESKID_EXECUTABLE_PROGRAM_RETURNS_VOID)
  beskid_program_main();
  int status = 0;
#else
  int status = (int)beskid_program_main();
#endif

  beskid_rt_v5_process_shutdown(runtime);
  return status;
}

#if defined(BESKID_EXECUTABLE_CORE_ARGS_UTF8)
int main(int argc, char **argv) {
  beskid_rt_v5_args_handoff_utf8((int64_t)argc, (const char *const *)argv);
  return beskid_execute_program();
}
#elif defined(BESKID_EXECUTABLE_CORE_ARGS_UTF16)
int wmain(int argc, wchar_t **argv) {
  beskid_rt_v5_args_handoff_utf16((int64_t)argc,
                                  (const uint16_t *const *)argv);
  return beskid_execute_program();
}
#else
int main(void) { return beskid_execute_program(); }
#endif
