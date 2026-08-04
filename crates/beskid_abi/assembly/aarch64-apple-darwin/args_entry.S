.text
.globl _main
.p2align 2
_main:
    mov x19, x30
    bl _beskid_rt_v5_args_handoff_utf8
    mov x30, x19
    b _beskid_program_main
