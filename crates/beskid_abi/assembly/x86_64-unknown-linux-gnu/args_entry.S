.text
.globl main
.type main, @function
main:
    subq $8, %rsp
    call beskid_rt_v5_args_handoff_utf8
    addq $8, %rsp
    jmp beskid_program_main
.size main, .-main
.section .note.GNU-stack,"",@progbits
