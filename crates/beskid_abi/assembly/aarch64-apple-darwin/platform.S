.text
.p2align 2

// The canonical source runtime calls these three target-bound operations directly. They are
// intentionally the only non-context assembly in this artifact: all portable pointer and memory
// primitives are lowered by codegen rather than exported by a host bridge.

.globl _beskid_rt_v5_intrinsic_system_allocate
_beskid_rt_v5_intrinsic_system_allocate:
    // mmap allocations are page aligned (16 KiB on supported Apple Silicon hosts). Reject an
    // unsupported stricter request rather than returning an insufficiently aligned pointer.
    mov x9, #16384
    cmp x1, x9
    b.hi L_allocate_failed
    cbz x0, L_allocate_failed
    mov x10, #16383
    adds x1, x0, x10
    b.cs L_allocate_failed
    and x1, x1, #0xffffffffffffc000
    mov x0, xzr
    mov x2, #3
    mov x3, #0x1002
    mov x4, #-1
    mov x5, xzr
    stp x29, x30, [sp, #-16]!
    bl _mmap
    ldp x29, x30, [sp], #16
    cmn x0, #1
    csel x0, xzr, x0, eq
    ret

L_allocate_failed:
    mov x0, xzr
    ret

.globl _beskid_rt_v5_intrinsic_system_free
_beskid_rt_v5_intrinsic_system_free:
    cbz x0, L_free_done
    cbz x1, L_free_done
    mov x9, #16383
    adds x1, x1, x9
    b.cs L_free_done
    and x1, x1, #0xffffffffffffc000
    stp x29, x30, [sp, #-16]!
    bl _munmap
    ldp x29, x30, [sp], #16
L_free_done:
    ret

.globl _beskid_rt_v5_trap
_beskid_rt_v5_trap:
    // ABI-v5 fixes the process status at 101. The code is diagnostic classification only;
    // `write` receives the manifest-provided message bytes before the non-returning exit.
    mov x0, #2
    // x1 and x2 already carry message/message_len for write.
    bl _write
    mov x0, #101
    bl __exit
    brk #0

.subsections_via_symbols
