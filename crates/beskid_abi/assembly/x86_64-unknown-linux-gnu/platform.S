.text
.p2align 4

// Native Linux ownership for the runtime allocation boundary. The canonical source runtime
// handles portable memory operations; this object only maps and unmaps page-backed storage.

.globl beskid_rt_v5_intrinsic_system_allocate
.type beskid_rt_v5_intrinsic_system_allocate, @function
beskid_rt_v5_intrinsic_system_allocate:
    // mmap returns page-aligned storage. Reject a stricter alignment request instead of
    // returning storage that violates the ABI contract.
    cmpq $4096, %rsi
    ja .Lallocate_failed
    testq %rdi, %rdi
    jz .Lallocate_failed
    movq %rdi, %rsi
    addq $4095, %rsi
    jc .Lallocate_failed
    andq $-4096, %rsi
    xorl %edi, %edi
    movl $3, %edx
    movl $0x22, %ecx
    movl $-1, %r8d
    xorl %r9d, %r9d
    subq $8, %rsp
    call mmap@PLT
    addq $8, %rsp
    cmpq $-1, %rax
    je .Lallocate_failed
    ret
.Lallocate_failed:
    xorl %eax, %eax
    ret
.size beskid_rt_v5_intrinsic_system_allocate, .-beskid_rt_v5_intrinsic_system_allocate

.globl beskid_rt_v5_intrinsic_system_free
.type beskid_rt_v5_intrinsic_system_free, @function
beskid_rt_v5_intrinsic_system_free:
    testq %rdi, %rdi
    jz .Lfree_done
    testq %rsi, %rsi
    jz .Lfree_done
    addq $4095, %rsi
    jc .Lfree_done
    andq $-4096, %rsi
    subq $8, %rsp
    call munmap@PLT
    addq $8, %rsp
.Lfree_done:
    ret
.size beskid_rt_v5_intrinsic_system_free, .-beskid_rt_v5_intrinsic_system_free

.section .note.GNU-stack,"",@progbits
