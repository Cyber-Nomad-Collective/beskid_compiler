# Core.Threading

`Core.Threading` provides preemptive OS-thread wrappers via the platform's native pthread library. The module exposes `Thread` handles, `Spawn`/`Join`/`Yield` operations, and `ThreadError` for spawn failures. This is distinct from cooperative concurrency (`Concurrency.Yield`).

## Types

| Type | Role |
|------|------|
| `Thread` | Opaque handle wrapping a `pthread_t` identifier. |
| `ThreadError` | `SpawnFailed(i64 code)` — the host pthread return code when thread creation is rejected. |

## Functions

| Function | Signature | Behavior |
|----------|-----------|----------|
| `Spawn` | `Result<Thread, ThreadError> Spawn(i64 entryRoutine, i64 arg)` | Creates an OS thread via `pthread_create`. Falls back from Linux (`libc.so.6`) to macOS (`libc`) if the first call fails. Returns the thread handle or `ThreadError::SpawnFailed(code)`. |
| `Join` | `Result<i64, ThreadError> Join(Thread self)` | Blocks until the thread exits via `pthread_join`. Returns the exit value or `SpawnFailed(code)` on error. |
| `Yield` | `unit Yield()` | Calls `sched_yield()` to hint the OS scheduler. |

## Starting a thread

```beskid
Core.Results.Result<Thread, ThreadError> handle =
    Core.Threading.Thread.Spawn(myEntryFn, 0);

match handle {
    Result::Ok(thread) => {
        // thread is running
    },
    Result::Error(err) => {
        // err.code contains the pthread error
    },
}
```

## Joining a thread

```beskid
Core.Results.Result<i64, ThreadError> exit =
    Core.Threading.Thread.Join(thread);

i64 exitCode = match exit {
    Result::Ok(code) => code,
    Result::Error(_) => -1,
};
```

## Yielding

```beskid
Core.Threading.Thread.Yield();
// Hints the OS scheduler — does not block or switch to Beskid's
// cooperative scheduler.
```

## Common patterns

**Spawning multiple workers:**

```beskid
Thread workers[4];
i64 i = 0;
while i < 4 {
    Result<Thread, ThreadError> spawned = Thread.Spawn(workerEntry, i);
    match spawned {
        Result::Ok(t) => {
            workers[i] = t;
        },
        Result::Error(_) => {
            // handle failure — corelib does not retry
        },
    }
    i = i + 1;
}
```

## Gotchas

- `Spawn` takes raw `i64` entry-point and argument values. Beskid's runtime attaches the language context before user code runs, but incorrect addresses produce undefined behavior at the OS level.
- `Join` may return `SpawnFailed` as its error variant even though the actual failure was in joining — the error enum currently has only one variant.
- The module attempts Linux `libc.so.6` first, then macOS `libc` — other platforms (Windows, WASM) are unsupported in v1.
- This module is **not** re-exported from `Prelude.bd`; import `Core.Threading` explicitly.
