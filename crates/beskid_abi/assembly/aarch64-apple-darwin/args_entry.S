.text
.globl _main
.p2align 2
_main:
    bl _beskid_rt_v5_args_handoff_utf8
    b _beskid_program_main
