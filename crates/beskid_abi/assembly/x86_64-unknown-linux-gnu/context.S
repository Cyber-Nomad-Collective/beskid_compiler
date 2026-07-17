#include "beskid_runtime_abi_v5_x86_64_unknown_linux_gnu.inc"

#ifdef __BESKID_TEST_MACHO
#define BESKID_SYMBOL(name) _ ## name
#define BESKID_FUNCTION(name)
#define BESKID_FUNCTION_SIZE(name)
#else
#define BESKID_SYMBOL(name) name
#define BESKID_FUNCTION(name) .type name, @function
#define BESKID_FUNCTION_SIZE(name) .size name, .-name
#endif

.text
.p2align 4
.globl BESKID_SYMBOL(beskid_arch_v5_context_init)
BESKID_FUNCTION(beskid_arch_v5_context_init)
BESKID_SYMBOL(beskid_arch_v5_context_init):
    movq $0, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RBX_OFFSET(%rdi)
    movq $0, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RBP_OFFSET(%rdi)
    movq %rcx, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R12_OFFSET(%rdi)
    movq $0, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R13_OFFSET(%rdi)
    movq $0, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R14_OFFSET(%rdi)
    movq $0, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R15_OFFSET(%rdi)
    movq %rsi, %rax
    andq $-16, %rax
    subq $8, %rax
    movq %r8, (%rax)
    movq %rax, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RSP_OFFSET(%rdi)
    movq %rdx, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RIP_OFFSET(%rdi)
    ret
BESKID_FUNCTION_SIZE(beskid_arch_v5_context_init)

.p2align 4
.globl BESKID_SYMBOL(beskid_arch_v5_context_switch)
BESKID_FUNCTION(beskid_arch_v5_context_switch)
BESKID_SYMBOL(beskid_arch_v5_context_switch):
    movq %rbx, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RBX_OFFSET(%rdi)
    movq %rbp, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RBP_OFFSET(%rdi)
    movq %r12, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R12_OFFSET(%rdi)
    movq %r13, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R13_OFFSET(%rdi)
    movq %r14, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R14_OFFSET(%rdi)
    movq %r15, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R15_OFFSET(%rdi)
    movq %rsp, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RSP_OFFSET(%rdi)
    leaq L_beskid_context_resume(%rip), %rax
    movq %rax, BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RIP_OFFSET(%rdi)

    movq %rsi, %r10
    movq BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RBX_OFFSET(%r10), %rbx
    movq BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RBP_OFFSET(%r10), %rbp
    movq BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R12_OFFSET(%r10), %r12
    movq BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R13_OFFSET(%r10), %r13
    movq BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R14_OFFSET(%r10), %r14
    movq BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_R15_OFFSET(%r10), %r15
    movq BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RSP_OFFSET(%r10), %rsp
    movq BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_RIP_OFFSET(%r10), %r11
    movq %r12, %rdi
    jmp *%r11

L_beskid_context_resume:
    ret
BESKID_FUNCTION_SIZE(beskid_arch_v5_context_switch)

#ifndef __BESKID_TEST_MACHO
.section .note.GNU-stack,"",@progbits
#endif
