OPTION CASEMAP:NONE
.code
EXTERN beskid_rt_v5_args_handoff_utf16:PROC
EXTERN beskid_program_main:PROC
PUBLIC wmain
wmain PROC
    sub rsp, 40
    call beskid_rt_v5_args_handoff_utf16
    add rsp, 40
    jmp beskid_program_main
wmain ENDP
END
