; Native Windows ownership for the ABI-v5 allocation boundary. Portable memory operations remain
; compiler intrinsics; this COFF object only reserves/releases virtual memory.

OPTION PROLOGUE:NONE
OPTION EPILOGUE:NONE

EXTERN VirtualAlloc:PROC
EXTERN VirtualFree:PROC
INCLUDELIB kernel32.lib

.code

PUBLIC beskid_rt_v5_intrinsic_system_allocate
beskid_rt_v5_intrinsic_system_allocate PROC
    ; VirtualAlloc reserves 64 KiB-granularity regions. Reject requests that demand a stricter
    ; alignment rather than returning a pointer that violates the ABI allocation contract.
    test rcx, rcx
    jz allocate_failed
    cmp rdx, 65536
    ja allocate_failed
    mov rdx, rcx
    xor rcx, rcx
    mov r8d, 3000h ; MEM_COMMIT | MEM_RESERVE
    mov r9d, 4     ; PAGE_READWRITE
    sub rsp, 40
    call VirtualAlloc
    add rsp, 40
    ret
allocate_failed:
    xor eax, eax
    ret
beskid_rt_v5_intrinsic_system_allocate ENDP

PUBLIC beskid_rt_v5_intrinsic_system_free
beskid_rt_v5_intrinsic_system_free PROC
    test rcx, rcx
    jz free_done
    xor rdx, rdx
    mov r8d, 8000h ; MEM_RELEASE
    sub rsp, 40
    call VirtualFree
    add rsp, 40
free_done:
    ret
beskid_rt_v5_intrinsic_system_free ENDP

END
