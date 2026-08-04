.text
.globl _main
.p2align 2
_main:
    stp x19, x30, [sp, #-16]!
    bl _beskid_rt_v5_args_handoff_utf8
    ldp x19, x30, [sp], #16
    b _beskid_program_main
