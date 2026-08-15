use std::{fs, path::PathBuf};

fn compiler_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn core_fs_uses_only_compiler_minted_services() {
    let source = fs::read_to_string(compiler_root().join("corelib/packages/foundation/src/Core/FS/FS.bd")).unwrap();
    for service in ["__fs_read_text", "__fs_write_text", "__fs_exists", "__fs_mkdir", "__fs_delete"] {
        assert!(source.contains(service), "missing Corelib service call {service}");
    }
    for intrinsic in ["fs_read_text(", "fs_write_text(", "fs_exists(", "fs_mkdir(", "fs_delete("] {
        assert!(!source.replace("__fs_", "service_").contains(intrinsic), "Core.FS directly calls {intrinsic}");
    }
    assert!(source.contains("Result<unit, FsError>"));
    assert!(source.contains("0 => Result::Ok(true)"));
    assert!(source.contains("1 => Result::Ok(false)"));
}

#[test]
fn canonical_runtime_fs_wrapper_owns_privileged_intrinsics() {
    let source = fs::read_to_string(compiler_root().join("runtime/beskid/src/Runtime/Host/FS.bd")).unwrap();
    for intrinsic in ["fs_read_text", "fs_read_text_release", "fs_write_text", "fs_exists", "fs_mkdir", "fs_delete"] {
        assert!(source.contains(intrinsic));
    }
    assert!(source.contains("pointer text = StrNew(bytes, length);"));
    let copy = source.find("pointer text = StrNew(bytes, length);").unwrap();
    let release = source.find("fs_read_text_release(bytes, length);").unwrap();
    let publish = source.find("raw_word_store(textOut, raw_word_load(text));").unwrap();
    assert!(copy < release && release < publish, "native bytes must be copied and released before publication");
    let process = fs::read_to_string(compiler_root().join("runtime/beskid/src/Runtime/Host/Process.bd")).unwrap();
    assert!(!process.contains("fs_read_text"));
}

#[test]
fn every_manifest_target_binding_has_an_exact_c_implementation() {
    let root = compiler_root();
    for (target, prefix) in [
        ("x86_64-unknown-linux-gnu", "linux"),
        ("aarch64-apple-darwin", "darwin"),
        ("x86_64-pc-windows-msvc", "windows"),
    ] {
        let source =
            fs::read_to_string(root.join("crates/beskid_abi/assembly").join(target).join("platform_host.c")).unwrap();
        for operation in ["read_text", "write_text", "exists", "mkdir", "delete"] {
            let symbol = format!("beskid_rt_v5_{prefix}_fs_{operation}");
            assert!(source.contains(&symbol), "missing {symbol}");
        }
    }
}

#[test]
fn syscall_workers_never_receive_managed_buffers_or_allocate_managed_results() {
    let root = compiler_root();
    let syscall = fs::read_to_string(root.join("runtime/beskid/src/Runtime/Io/Syscalls.bd")).unwrap();
    assert!(syscall.contains("memory_copy(nativeBuffer, managedBuffer, len)"));
    assert!(syscall.contains("FiberState::Parked"));
    assert!(syscall.contains("SchedulerRegisterWorkerRequest"));
    assert!(syscall.contains("if operation == SYSCALL_READ && result > 0"));
    let submit = syscall.find("WorkerSubmit(request)").unwrap();
    let read_copy = syscall.find("memory_copy(managedBuffer, nativeBuffer").unwrap();
    assert!(submit < read_copy, "managed read publication must happen only after resume");

    let rollback = syscall.find("SchedulerUnregisterWorkerRequest(request);").unwrap();
    let failed_submit_free = syscall[rollback..].find("SystemFree(request, SYSCALL_REQUEST_SIZE);").unwrap() + rollback;
    assert!(
        submit < rollback && rollback < failed_submit_free,
        "failed submission must unregister before freeing the request"
    );
    let scheduler = fs::read_to_string(root.join("runtime/beskid/src/Runtime/Fiber/Scheduler/Core.bd")).unwrap();
    assert!(scheduler.contains("pub bool SchedulerUnregisterWorkerRequest(pointer request)"));
    assert!(scheduler.contains("if raw_word_load(record) == NativeWord(request)"));

    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let source =
            fs::read_to_string(root.join("crates/beskid_abi/assembly").join(target).join("platform_host.c")).unwrap();
        assert!(source.contains("beskid_worker_main"));
        assert!(source.contains("beskid_rt_v5_intrinsic_worker_pool_shutdown"));
        assert!(!source.contains("beskid_rt_v5_thread_attach("));
        assert!(!source.contains("beskid_rt_v5_managed_object_allocate("));
    }
}

#[test]
fn worker_requests_preserve_native_handles_and_validate_operations_before_queueing() {
    let root = compiler_root();
    let manifest = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    assert!(manifest.contains("{ name = native_handle, offset = 24, type = usize }"));
    assert!(!manifest.contains("{ name = fd, offset = 24, type = i32 }"));

    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let source =
            fs::read_to_string(root.join("crates/beskid_abi/assembly").join(target).join("platform_host.c")).unwrap();
        assert!(source.contains("uintptr_t native_handle;"), "{target} must use a pointer-width native handle");
        assert!(!source.contains("int32_t fd;"), "{target} must not truncate the native handle");
    }

    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        let source =
            fs::read_to_string(root.join("crates/beskid_abi/assembly").join(target).join("platform_host.c")).unwrap();
        assert!(source.contains("if (r->native_handle > INT_MAX)"));
        assert!(source.contains("int fd = (int)r->native_handle;"));
    }

    let windows =
        fs::read_to_string(root.join("crates/beskid_abi/assembly/x86_64-pc-windows-msvc/platform_host.c")).unwrap();
    let submit = windows.find("int32_t beskid_rt_v5_intrinsic_worker_submit").unwrap();
    let validate = windows[submit..].find("r->operation != BESKID_WORKER_READ").unwrap() + submit;
    let queue =
        windows[submit..].find("InterlockedCompareExchangePointer((PVOID volatile *)&beskid_requests[i]").unwrap()
            + submit;
    assert!(validate < queue, "Windows must reject an invalid operation before queue publication");
    assert!(windows.contains("r->error = ERROR_INVALID_FUNCTION;"));
    assert!(windows.contains("HANDLE h = (HANDLE)r->native_handle;"));
}

#[test]
fn read_adapters_publish_output_only_after_success() {
    let root = compiler_root();
    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let source =
            fs::read_to_string(root.join("crates/beskid_abi/assembly").join(target).join("platform_host.c")).unwrap();
        assert!(
            !source.contains("struct BeskidStr *text_out"),
            "{target} must not publish unmanaged storage as a string"
        );
        let publish = source.find("*bytes_out = bytes;").expect("read adapter must publish temporary bytes");
        let length = source[publish..].find("*length_out = ").expect("read adapter must publish byte length");
        let success = source[publish + length..]
            .find("return BESKID_FS_OK")
            .or_else(|| source[publish + length..].find("return 0"));
        assert!(success.is_some(), "{target} must publish raw output immediately before success");
        assert!(source.contains("*bytes_out = NULL;") && source.contains("*length_out = 0;"));
        assert!(source.contains("fs_read_text_release"), "{target} must release temporary read storage");
        assert!(
            source.contains("len ? len : 1") || source.contains("length == 0 ? 1 : length"),
            "{target} must allocate addressable storage for empty reads"
        );
    }
}
