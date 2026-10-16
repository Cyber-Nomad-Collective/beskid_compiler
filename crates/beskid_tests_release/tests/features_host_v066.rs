//! PAR066-02, PORT066-02, PSEQ066-02, MOVE066-01 and GCS066-01: the 0.6.6 language and runtime
//! features through the installed CLI (`language-meta--evaluation--fibers-and-spawn`,
//! `language-meta--memory-model--memory-and-references`,
//! `execution--runtime--memory-and-gc-runtime-contract`).
//!
//! Each case writes one small application in a private home, runs it with `beskid run` and reads
//! the program's exit status. The parallel programs run twice: with `BESKID_ISOLATES_MAX=1` (every
//! spawn falls back to a fiber of the calling isolate) and with the default bound (the host
//! processor count). The Corelib `ParallelSuite` target covers the same libraries natively under
//! the default bound (PAR066-01, PORT066-01, PSEQ066-01).
use beskid_tests_release::installed::{Cli, Home, transcript};
use beskid_tests_release::project::write_app;
use std::path::Path;
use std::process::Output;

const ISOLATES_MAX: &str = "BESKID_ISOLATES_MAX";
const GC_STRESS: &str = "BESKID_GC_STRESS";

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

/// `beskid run --offline --plain --project <manifest>` with `environment` added.
fn run_app(cli: &Cli, home: &Home, manifest: &Path, environment: &[(&str, &str)]) -> Output {
    let mut command = cli.command(home);
    command.args(["run", "--offline", "--plain", "--project", path_text(manifest)]);
    command.env_remove(ISOLATES_MAX).env_remove(GC_STRESS);
    for (name, value) in environment {
        command.env(name, value);
    }
    cli.run(&mut command)
}

fn host_parallelism() -> usize {
    std::thread::available_parallelism().map(usize::from).unwrap_or(1)
}

/// Runs `source` at `BESKID_ISOLATES_MAX=1` and at the default bound; every run must exit 0.
fn assert_passes_at_one_isolate_and_default(name: &str, source: &str) {
    let cli = Cli::qualified();
    let home = Home::new();
    let manifest = write_app(&home.work(), name, source);
    let serial = run_app(cli, &home, &manifest, &[(ISOLATES_MAX, "1")]);
    assert_eq!(serial.status.code(), Some(0), "{name} at {ISOLATES_MAX}=1:\n{}", transcript(&serial));
    let default = run_app(cli, &home, &manifest, &[]);
    assert_eq!(default.status.code(), Some(0), "{name} at the default isolate bound:\n{}", transcript(&default));
}

// ---------------------------------------------------------------------------------------------
// PAR066-02 Fork-join scope: Spawn/Join results at one isolate and at the default bound

/// Exit 0: the scope sums 20 + 22 and the placement task ran on the calling (main) isolate. Exit 10:
/// the sum is right and the placement task ran on another isolate. Exit 1: a wrong or failed result.
const SCOPE_PROGRAM: &str = r#"use Core.Results;
use Core.Parallel;
use Core.Parallel.Scopes;
use Core.Parallel.Isolate;

i64 OrZero(Result<i64, TaskError> joined) {
    return match joined {
        Result::Ok(value) => value,
        Result::Error(_) => 0_i64,
    };
}

i64 OnMainCode(bool onMain) {
    if onMain {
        return 1000_i64;
    }
    return 0_i64;
}

i64 Placement(Result<bool, TaskError> joined) {
    return match joined {
        Result::Ok(onMain) => OnMainCode(onMain),
        Result::Error(_) => -100_i64,
    };
}

i64 ScopeValue(Result<i64, ParallelError> result) {
    return match result {
        Result::Ok(value) => value,
        Result::Error(_) => -1_i64,
    };
}

i32 Main() {
    Isolate caller = Isolate.Main();
    Result<i64, ParallelError> result = Parallel.Scope<i64>((Core.Parallel.Scopes.Scope s) => {
        Task<bool> placement = s.Spawn(() => Isolate.Current().Equals(caller));
        Task<i64> left = s.Spawn(() => 20_i64);
        Task<i64> right = s.Spawn(() => 22_i64);
        i64 placed = Placement(placement.Join());
        return Result::Ok(OrZero(left.Join()) + OrZero(right.Join()) + placed);
    });
    i64 value = ScopeValue(result);
    if value == 1042_i64 {
        return 0;
    }
    if value == 42_i64 {
        return 10;
    }
    return 1;
}
"#;

#[test]
fn par066_02_scope_spawn_join_result_at_one_isolate_and_default() {
    let cli = Cli::qualified();
    let home = Home::new();
    let manifest = write_app(&home.work(), "ForkJoin", SCOPE_PROGRAM);
    let serial = run_app(cli, &home, &manifest, &[(ISOLATES_MAX, "1")]);
    assert_eq!(
        serial.status.code(),
        Some(0),
        "at {ISOLATES_MAX}=1 the scope joins 42 and every task runs on the main isolate (exit 10: a task left the main isolate):\n{}",
        transcript(&serial)
    );
    let default = run_app(cli, &home, &manifest, &[]);
    let expected = if host_parallelism() >= 2 { 10 } else { 0 };
    assert_eq!(
        default.status.code(),
        Some(expected),
        "at the default bound ({} processors) the scope joins 42 and the task runs on {} isolate:\n{}",
        host_parallelism(),
        if expected == 10 { "another" } else { "the main" },
        transcript(&default)
    );
}

// ---------------------------------------------------------------------------------------------
// PORT066-02 Ports and pipelines: bounded producer/consumer and ordered stages

/// Exit 1: the bounded port lost or duplicated a value. Exit 2: the ordered pipeline consumed a
/// wrong record count or delivered records out of input order.
const PORT_PROGRAM: &str = r#"use Core.Results;
use Concurrency;
use Concurrency.TryResult;
use Core.Parallel;
use Core.Parallel.Scopes;
use Core.Parallel.Port;

i64 ScopeValue(Result<i64, ParallelError> result) {
    return match result {
        Result::Ok(value) => value,
        Result::Error(_) => -1_i64,
    };
}

i64 BoundedTotal() {
    Result<i64, ParallelError> result = Parallel.Scope<i64>((Core.Parallel.Scopes.Scope s) => {
        Port<i64> values = s.PortWith<i64>(4_i64);
        Task<unit> producer = s.Spawn(() => {
            mut i64 i = 0_i64;
            while i < 100_i64 {
                values.Send(i);
                i = i + 1_i64;
            }
            values.Close();
            return;
        });
        mut i64 total = 0_i64;
        mut bool open = true;
        while open {
            match values.Receive() {
                Result::Ok(v) => { total = total + v; },
                Result::Error(_) => { open = false; },
            };
        }
        return Result::Ok(total);
    });
    return ScopeValue(result);
}

i64 OrderedPipeline() {
    Result<i64, ParallelError> result = Parallel.Scope<i64>((Core.Parallel.Scopes.Scope s) => {
        Port<string> lines = s.PortWith<string>(2000_i64);
        Result<i64, ParallelError> consumed = s
            .Source((Port<i64> out) => {
                mut i64 i = 1_i64;
                while i <= 1000_i64 {
                    out.Send(i);
                    i = i + 1_i64;
                }
                return;
            })
            .Stage(4_i64, (n) => n * n)
            .Ordered()
            .Stage(2_i64, (sq) => "${sq}")
            .Ordered()
            .Sink((line) => {
                lines.Send(line);
                return;
            });
        mut i64 expected = 1_i64;
        mut bool inOrder = true;
        mut bool more = true;
        while more {
            match lines.TryReceive() {
                TryResult::Ok(line) => {
                    i64 square = expected * expected;
                    if line != "${square}" {
                        inOrder = false;
                    }
                    expected = expected + 1_i64;
                },
                _ => { more = false; },
            };
        }
        if !inOrder {
            return Result::Ok(-2_i64);
        }
        if expected != 1001_i64 {
            return Result::Ok(-3_i64);
        }
        return consumed;
    });
    return ScopeValue(result);
}

i32 Main() {
    if BoundedTotal() != 4950_i64 {
        return 1;
    }
    if OrderedPipeline() != 1000_i64 {
        return 2;
    }
    return 0;
}
"#;

#[test]
fn port066_02_bounded_port_and_ordered_pipeline_at_one_isolate_and_default() {
    assert_passes_at_one_isolate_and_default("Ports", PORT_PROGRAM);
}

// ---------------------------------------------------------------------------------------------
// PSEQ066-02 Parallel sequences: Map/Reduce and Collect equal the serial result

/// Exit 1/2: Where/Map/Reduce differs from the serial loop or the known sum. Exit 3/4: Collect lost
/// an element or changed the order. Exit 5: the ordered Reduce changed the association order.
const PSEQ_PROGRAM: &str = r#"use Core.Optional.Option;
use Core.Collections.Array;
use Core.Collections.List;
use Core.Parallel;
use Core.Parallel.Combiners;
use Core.Parallel.ParallelSeq;

List<i64> Numbers(i64 count) {
    mut List<i64> values = List.New<i64>();
    mut i64 i = 0_i64;
    while i < count {
        values = values.Push(i);
        i = i + 1_i64;
    }
    return values;
}

i64 SerialSumOfSquaresOfEvens(List<i64> values) {
    mut i64 total = 0_i64;
    mut i64 i = 0_i64;
    while i < values.Count() {
        i64 x = match values.TryGet(i) { Option::Some(v) => v, Option::None => 0_i64 };
        if x % 2_i64 == 0_i64 {
            total = total + x * x;
        }
        i = i + 1_i64;
    }
    return total;
}

i32 Main() {
    List<i64> values = Numbers(1000_i64);
    i64 parallel = values
        .Parallel()
        .Where((x) => x % 2_i64 == 0_i64)
        .Map((x) => x * x)
        .Reduce(0_i64, SumI64 { });
    if parallel != SerialSumOfSquaresOfEvens(values) {
        return 1;
    }
    if parallel != 166167000_i64 {
        return 2;
    }
    List<i64> many = Numbers(10000_i64);
    List<i64> doubled = many.Parallel().Chunk(7_i64).Map((x) => x * 2_i64).Collect();
    if doubled.Count() != 10000_i64 {
        return 3;
    }
    mut i64 i = 0_i64;
    while i < doubled.Count() {
        i64 v = match doubled.TryGet(i) { Option::Some(x) => x, Option::None => -1_i64 };
        if v != 2_i64 * i {
            return 4;
        }
        i = i + 1_i64;
    }
    mut string[] words = Array.Empty<string>();
    words = Array.Append<string>(words, "a");
    words = Array.Append<string>(words, "b");
    words = Array.Append<string>(words, "c");
    words = Array.Append<string>(words, "d");
    string joined = Parallel.OfArray<string>(words).Chunk(1_i64).Reduce("", Concat { });
    if joined != "abcd" {
        return 5;
    }
    return 0;
}
"#;

#[test]
fn pseq066_02_parallel_map_reduce_equals_serial_at_one_isolate_and_default() {
    assert_passes_at_one_isolate_and_default("ParallelSequences", PSEQ_PROGRAM);
}

// ---------------------------------------------------------------------------------------------
// MOVE066-01 move and exchange, and use after move (E1234)

/// Exit 1: `exchange` did not yield the old buffer. Exit 2: it did not store the new one. Exit 3:
/// `move stage.buffer` did not leave the empty array. Exit 4: the moved buffer lost elements.
const MOVE_PROGRAM: &str = r#"use Core.Collections.Array;

type Stage {
    i64[] buffer,
}

i64[] Fill(i64 count) {
    mut i64[] values = Array.Empty<i64>();
    mut i64 i = 0_i64;
    while i < count {
        values = Array.Append<i64>(values, i);
        i = i + 1_i64;
    }
    return values;
}

i64 Consume(move i64[] values) {
    return Array.Len<i64>(values);
}

i32 Main() {
    mut Stage stage = Stage { buffer: Fill(3_i64) };
    i64[] next = Fill(5_i64);
    i64[] old = exchange(stage.buffer, move next);
    if Array.Len<i64>(old) != 3_i64 {
        return 1;
    }
    if Array.Len<i64>(stage.buffer) != 5_i64 {
        return 2;
    }
    i64[] taken = move stage.buffer;
    if Array.Len<i64>(stage.buffer) != 0_i64 {
        return 3;
    }
    if Consume(move taken) != 5_i64 {
        return 4;
    }
    return 0;
}
"#;

/// `data` is read after `move data` consumed it.
const USE_AFTER_MOVE_PROGRAM: &str = r#"use Core.Collections.Array;

i64 Consume(move i64[] values) {
    return Array.Len<i64>(values);
}

i32 Main() {
    i64[] data = Array.Empty<i64>();
    i64 first = Consume(move data);
    i64 second = Array.Len<i64>(data);
    return 0;
}
"#;

#[test]
fn move066_01_move_and_exchange_run_and_use_after_move_is_e1234() {
    let cli = Cli::qualified();
    let home = Home::new();
    let manifest = write_app(&home.work(), "MoveExchange", MOVE_PROGRAM);
    let ran = run_app(cli, &home, &manifest, &[]);
    assert_eq!(ran.status.code(), Some(0), "move and exchange transfer and swap the buffers:\n{}", transcript(&ran));

    let manifest = write_app(&home.work(), "UseAfterMove", USE_AFTER_MOVE_PROGRAM);
    let checked =
        cli.run(cli.command(&home).args(["check", "--json", "--plain", "--offline", "--project", path_text(&manifest)]));
    assert!(!checked.status.success(), "a read after `move data` fails check:\n{}", transcript(&checked));
    let diagnostics = text(&checked.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .flat_map(|record| diagnostic_records(&record))
        .collect::<Vec<_>>();
    let read_line = USE_AFTER_MOVE_PROGRAM.lines().position(|line| line.contains("i64 second")).unwrap() + 1;
    let read_column = USE_AFTER_MOVE_PROGRAM.lines().nth(read_line - 1).unwrap().find("(data)").unwrap() + 2;
    let use_after_move = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "E1234")
        .collect::<Vec<_>>();
    assert_eq!(use_after_move.len(), 1, "exactly one E1234 record:\n{diagnostics:#?}\n{}", transcript(&checked));
    let record = use_after_move[0];
    assert_eq!(record["severity"], "error", "{record:#}");
    assert!(record["message"].as_str().is_some_and(|message| message.contains("data")), "names the moved binding: {record:#}");
    assert_eq!(
        (&record["span"]["start"]["line"], &record["span"]["start"]["column"]),
        (&serde_json::json!(read_line), &serde_json::json!(read_column)),
        "E1234 points at `data` in the read on line {read_line}: {record:#}"
    );
    let built = cli.run(cli.command(&home).args(["run", "--offline", "--plain", "--project", path_text(&manifest)]));
    assert!(!built.status.success(), "the rejected program never runs:\n{}", transcript(&built));
}

/// Every `diagnostic` object of one `--json` record.
fn diagnostic_records(value: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut found = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key == "diagnostic" && child.is_object() {
                    found.push(child.clone());
                } else {
                    found.extend(diagnostic_records(child));
                }
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|item| found.extend(diagnostic_records(item))),
        _ => {}
    }
    found
}

// ---------------------------------------------------------------------------------------------
// GCS066-01 GC stress smoke

/// Exit 1: a list of strings built under collection pressure lost its count or its last element.
const GC_PROGRAM: &str = r#"use Core.Optional.Option;
use Core.Collections.List;

List<string> Words(i64 count) {
    mut List<string> values = List.New<string>();
    mut i64 i = 0_i64;
    while i < count {
        values = values.Push("word-${i}");
        i = i + 1_i64;
    }
    return values;
}

i64 LastCode(string last) {
    if last != "word-2999" {
        return 2_i64;
    }
    return 0_i64;
}

i64 Check(List<string> words) {
    if words.Count() != 3000_i64 {
        return 1_i64;
    }
    return match words.TryGet(2999_i64) {
        Option::Some(last) => LastCode(last),
        Option::None => 3_i64,
    };
}

i32 Main() {
    mut i64 round = 0_i64;
    while round < 4_i64 {
        List<string> words = Words(3000_i64);
        if Check(words) != 0_i64 {
            return 1;
        }
        round = round + 1_i64;
    }
    return 0;
}
"#;

#[test]
fn gcs066_01_gc_stress_alloc_64_verify_smoke() {
    let cli = Cli::qualified();
    let home = Home::new();
    let manifest = write_app(&home.work(), "GcStress", GC_PROGRAM);
    let plain = run_app(cli, &home, &manifest, &[]);
    assert_eq!(plain.status.code(), Some(0), "the program passes without stress:\n{}", transcript(&plain));
    let stressed = run_app(cli, &home, &manifest, &[(GC_STRESS, "alloc:64,verify")]);
    assert_eq!(
        stressed.status.code(),
        Some(0),
        "{GC_STRESS}=alloc:64,verify collects every 64 allocations and verifies the heap before each collection:\n{}",
        transcript(&stressed)
    );
    // The setting reaches the program's runtime: a malformed value stops it at initialization.
    let malformed = run_app(cli, &home, &manifest, &[(GC_STRESS, "alloc:0")]);
    let combined = format!("{}{}", text(&malformed.stdout), text(&malformed.stderr));
    assert!(
        !malformed.status.success() && combined.contains(GC_STRESS),
        "a malformed {GC_STRESS} traps at runtime initialization and names the variable:\n{}",
        transcript(&malformed)
    );
}
