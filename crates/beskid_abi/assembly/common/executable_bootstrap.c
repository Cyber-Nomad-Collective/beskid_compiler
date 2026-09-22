#include "../../include/beskid_runtime_abi_v5.h"

/*
 * The executable host is the only owner of process-lifetime runtime activation.
 * It deliberately contains no Core.IO behaviour: Core.IO owns descriptor loops,
 * EOF, no-progress, and close semantics inside the Beskid runtime.
 */
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
