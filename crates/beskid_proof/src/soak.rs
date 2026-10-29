//! Language-server soak client.
//!
//! Drives `beskid_lsp` over stdio on copies of one or more corpora for a configurable time:
//! open files, random edits and undo, saves, queries, workspace refreshes and old-format locks
//! that must fail closed. The run passes when the server never exits, never sends `beskid/crashReport`,
//! writes no new file into the crash directory, answers no request with an internal error, never
//! leaves a request unanswered past the timeout, its RSS grows less than the limit between
//! the first and the last window, and the interactive requests sent after the initial workspace
//! index answer within their latency budgets.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::framing;
use crate::mutate::{MutationKind, mutate};
use crate::rng::Rng;

pub const CRASH_REPORT_METHOD: &str = "beskid/crashReport";
const INTERNAL_ERROR_CODE: i64 = -32603;
const UNSUPPORTED_LOCK_CODE: &str = "E3025";
const CURRENT_LOCK_HEADER: &str = "# Project.lock v3\n";
const OLD_LOCK_DIAGNOSTIC_WAIT: Duration = Duration::from_secs(15);
const SKIPPED_DIRECTORIES: [&str; 5] = ["obj", ".git", "target", "node_modules", ".beskid"];

// ---------------------------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SoakConfig {
    pub server: PathBuf,
    pub beskid_cli: Option<PathBuf>,
    pub corpora: Vec<PathBuf>,
    pub duration: Duration,
    pub home: Option<PathBuf>,
    pub work_dir: Option<PathBuf>,
    pub seed: u64,
    pub max_open: usize,
    pub tick: Duration,
    pub request_timeout: Duration,
    /// Longest a `beskid dev project lock` run may take before the soak fails.
    pub cli_timeout: Duration,
    /// Longest the server may leave its stdin undrained before the soak fails.
    pub drain_timeout: Duration,
    pub sample_interval: Duration,
    pub memory_growth_limit_percent: f64,
    pub latency_budgets: Vec<LatencyBudget>,
    pub report: Option<PathBuf>,
}

/// Latency limits for one request method, measured on requests sent after the initial workspace
/// index finished (the server reports `beskid/status` idle).
#[derive(Debug, Clone, PartialEq)]
pub struct LatencyBudget {
    pub method: String,
    pub p95: Duration,
    pub max: Duration,
}

/// Interactive requests and their default budgets: p95 under 500 ms, none over 5 s.
pub fn default_latency_budgets() -> Vec<LatencyBudget> {
    ["textDocument/hover", "textDocument/completion", "textDocument/documentSymbol", "textDocument/definition"]
        .into_iter()
        .map(|method| LatencyBudget {
            method: method.to_string(),
            p95: Duration::from_millis(500),
            max: Duration::from_secs(5),
        })
        .collect()
}

/// Parse `<method>=<p95>,<max>`, e.g. `textDocument/hover=500ms,5s`.
pub fn parse_latency_budget(text: &str) -> Result<LatencyBudget, String> {
    let (method, limits) = text.split_once('=').ok_or_else(|| format!("bad latency budget `{text}`"))?;
    let (p95, max) = limits.split_once(',').ok_or_else(|| format!("bad latency budget `{text}` (need p95,max)"))?;
    let budget =
        LatencyBudget { method: method.trim().to_string(), p95: parse_duration(p95)?, max: parse_duration(max)? };
    if budget.method.is_empty() || budget.p95 > budget.max {
        return Err(format!("bad latency budget `{text}`"));
    }
    Ok(budget)
}

pub const USAGE: &str = "\
beskid-soak: scripted language-server soak client

usage: beskid-soak --server <beskid_lsp> --corpus <dir> [--corpus <dir> ...] [options]

  --server <path>        beskid_lsp binary (default: beskid_lsp on PATH)
  --corpus <dir>         corpus root, repeatable; copied into a temporary workspace, originals untouched
  --duration <time>      run length: 90s, 30m, 2h (default 30m)
  --smoke                60-second CI mode (memory growth is reported but not enforced)
  --home <dir>           HOME for the server; crash directory is <dir>/.beskid/crash (default: fresh temp dir)
  --work-dir <dir>       where corpus copies and the server log go (default: fresh temp dir)
  --beskid <path>        beskid CLI; when given, each old-format lock is also refused by
                         `beskid dev project lock`, then deleted and written again
  --seed <n>             random seed (default 1)
  --max-open <n>         documents open at once (default 8)
  --tick-ms <n>          pause between actions (default 100)
  --request-timeout <t>  unanswered-request limit (default 120s)
  --cli-timeout <t>      limit for one `beskid dev project lock` run (default 120s)
  --drain-timeout <t>    fail when the server leaves its stdin unread this long (default 30s)
  --max-memory-growth <percent>  RSS growth limit, first window to last window (default 20)
  --latency-budget <method>=<p95>,<max>
                         latency limits for requests sent after the initial index, repeatable;
                         replaces the default for that method (default: hover, completion,
                         documentSymbol and definition each 500ms,5s; not enforced with --smoke)
  --report <file>        also write the JSON report here

exit status: 0 pass, 1 soak failure, 2 usage or setup error";

pub fn parse_duration(text: &str) -> Result<Duration, String> {
    let text = text.trim();
    let split = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    let (digits, unit) = text.split_at(split);
    let value: u64 = digits.parse().map_err(|_| format!("bad duration `{text}`"))?;
    let seconds = match unit {
        "ms" => return Ok(Duration::from_millis(value)),
        "" | "s" => value,
        "m" => value * 60,
        "h" => value * 3600,
        _ => return Err(format!("bad duration unit in `{text}` (use ms, s, m or h)")),
    };
    Ok(Duration::from_secs(seconds))
}

impl SoakConfig {
    /// Parse command-line arguments (without the program name).
    pub fn from_args<I: IntoIterator<Item = String>>(args: I) -> Result<(Self, bool), String> {
        let mut config = SoakConfig {
            server: PathBuf::from("beskid_lsp"),
            beskid_cli: None,
            corpora: Vec::new(),
            duration: Duration::from_secs(30 * 60),
            home: None,
            work_dir: None,
            seed: 1,
            max_open: 8,
            tick: Duration::from_millis(100),
            request_timeout: Duration::from_secs(120),
            cli_timeout: Duration::from_secs(120),
            drain_timeout: Duration::from_secs(30),
            sample_interval: Duration::from_secs(10),
            memory_growth_limit_percent: 20.0,
            latency_budgets: default_latency_budgets(),
            report: None,
        };
        let mut smoke = false;
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
            match flag.as_str() {
                "--server" => config.server = value("--server")?.into(),
                "--corpus" => config.corpora.push(value("--corpus")?.into()),
                "--duration" => config.duration = parse_duration(&value("--duration")?)?,
                "--smoke" => smoke = true,
                "--home" => config.home = Some(value("--home")?.into()),
                "--work-dir" => config.work_dir = Some(value("--work-dir")?.into()),
                "--beskid" => config.beskid_cli = Some(value("--beskid")?.into()),
                "--seed" => config.seed = value("--seed")?.parse().map_err(|_| "bad --seed".to_string())?,
                "--max-open" => {
                    config.max_open = value("--max-open")?.parse().map_err(|_| "bad --max-open".to_string())?
                }
                "--tick-ms" => {
                    config.tick =
                        Duration::from_millis(value("--tick-ms")?.parse().map_err(|_| "bad --tick-ms".to_string())?)
                }
                "--request-timeout" => config.request_timeout = parse_duration(&value("--request-timeout")?)?,
                "--cli-timeout" => config.cli_timeout = parse_duration(&value("--cli-timeout")?)?,
                "--drain-timeout" => config.drain_timeout = parse_duration(&value("--drain-timeout")?)?,
                "--max-memory-growth" => {
                    config.memory_growth_limit_percent =
                        value("--max-memory-growth")?.parse().map_err(|_| "bad --max-memory-growth".to_string())?
                }
                "--latency-budget" => {
                    let budget = parse_latency_budget(&value("--latency-budget")?)?;
                    config.latency_budgets.retain(|known| known.method != budget.method);
                    config.latency_budgets.push(budget);
                }
                "--report" => config.report = Some(value("--report")?.into()),
                "-h" | "--help" => return Err(USAGE.to_string()),
                other => return Err(format!("unknown argument `{other}`\n\n{USAGE}")),
            }
        }
        if config.corpora.is_empty() {
            return Err(format!("at least one --corpus is required\n\n{USAGE}"));
        }
        if config.max_open == 0 {
            return Err("--max-open must be at least 1".into());
        }
        if smoke {
            config.duration = Duration::from_secs(60);
        }
        Ok((config, smoke))
    }
}

// ---------------------------------------------------------------------------------------------
// Pure helpers (unit tested)
// ---------------------------------------------------------------------------------------------

/// One RSS sample: seconds since the run started, resident set size in KiB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RssSample {
    pub at: Duration,
    pub kib: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryVerdict {
    pub first_window_kib: u64,
    pub last_window_kib: u64,
    pub growth_percent: f64,
}

/// Mean RSS of the samples inside the first and the last `window` of the run and the growth
/// between them. The first window starts after a warm-up equal to the window (indexing is still
/// ramping up at t=0), so it covers `(window, 2 * window)`. `None` when either window holds no
/// sample or the first mean is zero.
pub fn memory_growth(samples: &[RssSample], total: Duration, window: Duration) -> Option<MemoryVerdict> {
    let mean = |select: &dyn Fn(&RssSample) -> bool| -> Option<u64> {
        let chosen: Vec<u64> = samples.iter().filter(|s| select(s)).map(|s| s.kib).collect();
        (!chosen.is_empty()).then(|| chosen.iter().sum::<u64>() / chosen.len() as u64)
    };
    let first = mean(&|s: &RssSample| s.at > window && s.at < window * 2)?;
    let last = mean(&|s: &RssSample| s.at + window >= total)?;
    if first == 0 {
        return None;
    }
    Some(MemoryVerdict {
        first_window_kib: first,
        last_window_kib: last,
        growth_percent: (last as f64 - first as f64) * 100.0 / first as f64,
    })
}

pub fn parse_rss_output(output: &str) -> Option<u64> {
    output.trim().parse().ok()
}

/// Count, median, 95th percentile and maximum of one method's latencies (nearest rank).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencySummary {
    pub count: usize,
    pub p50: Duration,
    pub p95: Duration,
    pub max: Duration,
}

pub fn latency_summary(samples: &[Duration]) -> Option<LatencySummary> {
    let mut sorted = samples.to_vec();
    sorted.sort();
    let rank = |q: f64| sorted[((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1];
    (!sorted.is_empty()).then(|| LatencySummary {
        count: sorted.len(),
        p50: rank(0.5),
        p95: rank(0.95),
        max: *sorted.last().expect("non-empty"),
    })
}

/// Failures of the latency budgets against the latencies measured after the initial index.
pub fn latency_failures(
    budgets: &[LatencyBudget],
    measured: &BTreeMap<String, Vec<Duration>>,
    index_finished: bool,
) -> Vec<String> {
    let mut failures = Vec::new();
    for budget in budgets {
        let Some(summary) = measured.get(&budget.method).and_then(|samples| latency_summary(samples)) else {
            failures.push(if index_finished {
                format!("no `{}` request was measured after the initial index", budget.method)
            } else {
                format!("the initial index did not finish, so `{}` latency was not measured", budget.method)
            });
            continue;
        };
        if summary.p95 > budget.p95 {
            failures.push(format!(
                "`{}` p95 {} ms over the {} ms budget ({} requests)",
                budget.method,
                summary.p95.as_millis(),
                budget.p95.as_millis(),
                summary.count
            ));
        }
        if summary.max > budget.max {
            failures.push(format!(
                "`{}` took {} ms, over the {} ms limit",
                budget.method,
                summary.max.as_millis(),
                budget.max.as_millis()
            ));
        }
    }
    failures
}

/// Files in `after` that were not in `before`.
pub fn new_entries(before: &BTreeSet<String>, after: &BTreeSet<String>) -> Vec<String> {
    after.difference(before).cloned().collect()
}

pub fn list_directory(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default()
}

pub fn path_to_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// An old-format lock for the project of a current (v3) lock: the v1 header with the same root
/// manifest and project name and no dependency lines, which the server and the CLI must refuse
/// with `E3025`. `None` when the content is not a current lock.
pub fn downgrade_lock_to_v1(current: &str) -> Option<String> {
    let rest = current.strip_prefix("# Project.lock v3\n")?;
    let header = rest.lines().take_while(|line| !line.starts_with("- ")).collect::<Vec<_>>().join("\n");
    Some(format!("# Project.lock v1\n{header}\n"))
}

/// Reply for a server-to-client request; the soak client supports nothing, so it answers neutrally.
pub fn answer_server_request(request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let result = match request.get("method").and_then(Value::as_str) {
        // One null per requested item.
        Some("workspace/configuration") => {
            let count = request.pointer("/params/items").and_then(Value::as_array).map_or(0, Vec::len);
            Value::Array(vec![Value::Null; count])
        }
        Some("workspace/workspaceFolders") => Value::Null,
        _ => Value::Null,
    };
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// `beskid/status` idle: the server finished its workspace scan.
pub fn is_index_idle(message: &Value) -> bool {
    message.get("method").and_then(Value::as_str) == Some("beskid/status")
        && message.pointer("/params/phase").and_then(Value::as_str) == Some("idle")
        && message.pointer("/params/active").and_then(Value::as_bool) == Some(false)
}

pub fn is_crash_report(message: &Value) -> bool {
    message.get("method").and_then(Value::as_str) == Some(CRASH_REPORT_METHOD) && message.get("id").is_none()
}

/// A response is an internal server error when it carries JSON-RPC code -32603.
pub fn is_internal_error_response(message: &Value) -> bool {
    message.get("method").is_none()
        && message.pointer("/error/code").and_then(Value::as_i64) == Some(INTERNAL_ERROR_CODE)
}

// ---------------------------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------------------------

const SNIPPETS: [&str; 10] = ["(", ")", "{", "}", ";", "mut ", "i32 x = ;", "return", "\"", "//"];

#[derive(Debug, Clone)]
pub struct Document {
    pub path: PathBuf,
    pub uri: String,
    pub text: String,
    pub version: i64,
    pub history: Vec<String>,
}

const HISTORY_LIMIT: usize = 50;

impl Document {
    pub fn new(path: PathBuf, text: String) -> Self {
        let uri = path_to_uri(&path);
        Self { path, uri, text, version: 1, history: Vec::new() }
    }

    /// Apply a random edit. Returns false when no edit applied.
    pub fn random_edit(&mut self, rng: &mut Rng) -> bool {
        let edited = if rng.chance(30) {
            // Insert a snippet at a random char boundary.
            let boundaries: Vec<usize> =
                self.text.char_indices().map(|(i, _)| i).chain(std::iter::once(self.text.len())).collect();
            let at = boundaries[rng.below(boundaries.len())];
            let snippet = SNIPPETS[rng.below(SNIPPETS.len())];
            Some(format!("{}{}{}", &self.text[..at], snippet, &self.text[at..]))
        } else {
            let kind = MutationKind::ALL[rng.below(MutationKind::ALL.len())];
            mutate(&self.text, kind, rng)
        };
        match edited {
            Some(text) => {
                self.push_history();
                self.text = text;
                self.version += 1;
                true
            }
            None => false,
        }
    }

    fn push_history(&mut self) {
        self.history.push(self.text.clone());
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
    }

    /// Restore the previous text. Returns false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        match self.history.pop() {
            Some(previous) => {
                self.text = previous;
                self.version += 1;
                true
            }
            None => false,
        }
    }

    /// A random (line, character) position that exists in the text.
    pub fn random_position(&self, rng: &mut Rng) -> (u32, u32) {
        let lines: Vec<&str> = self.text.split('\n').collect();
        let line = rng.below(lines.len());
        let width = lines[line].encode_utf16().count();
        (line as u32, rng.below(width + 1) as u32)
    }
}

// ---------------------------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct Counters {
    pub edits: u64,
    pub undos: u64,
    pub saves: u64,
    pub queries: u64,
    pub rotations: u64,
    pub refreshes: u64,
    pub old_locks: u64,
    pub old_lock_errors_seen: u64,
    pub responses: u64,
    pub error_responses: u64,
    pub diagnostics_published: u64,
}

#[derive(Debug, Default)]
pub struct SoakReport {
    pub duration: Duration,
    pub counters: Counters,
    pub failures: Vec<String>,
    pub crash_reports: Vec<String>,
    pub new_crash_files: Vec<String>,
    pub internal_errors: Vec<String>,
    pub memory: Option<MemoryVerdict>,
    pub memory_enforced: bool,
    pub samples: Vec<RssSample>,
    pub server_log: Option<PathBuf>,
    /// Seconds from the start of the run until the initial workspace index finished.
    pub index_finished_after: Option<Duration>,
    /// Response latencies of requests sent after the initial index, by method.
    pub latencies: BTreeMap<String, Vec<Duration>>,
    pub latency_enforced: bool,
}

impl SoakReport {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn to_json(&self) -> Value {
        let c = &self.counters;
        json!({
            "result": if self.passed() { "pass" } else { "fail" },
            "durationSeconds": self.duration.as_secs(),
            "failures": self.failures,
            "crashReportNotifications": self.crash_reports,
            "newCrashFiles": self.new_crash_files,
            "internalErrorResponses": self.internal_errors,
            "memory": self.memory.as_ref().map(|m| json!({
                "firstWindowKiB": m.first_window_kib,
                "lastWindowKiB": m.last_window_kib,
                "growthPercent": m.growth_percent,
                "enforced": self.memory_enforced,
            })),
            "rssSamples": self.samples.iter().map(|s| json!([s.at.as_secs(), s.kib])).collect::<Vec<_>>(),
            "indexFinishedAfterSeconds": self.index_finished_after.map(|at| at.as_secs_f64()),
            "latencyEnforced": self.latency_enforced,
            "latencyMs": self.latencies.iter().filter_map(|(method, samples)| {
                let summary = latency_summary(samples)?;
                Some((method.clone(), json!({
                    "count": summary.count,
                    "p50": summary.p50.as_secs_f64() * 1000.0,
                    "p95": summary.p95.as_secs_f64() * 1000.0,
                    "max": summary.max.as_secs_f64() * 1000.0,
                })))
            }).collect::<serde_json::Map<String, Value>>(),
            "serverLog": self.server_log.as_ref().map(|p| p.display().to_string()),
            "actions": {
                "edits": c.edits, "undos": c.undos, "saves": c.saves, "queries": c.queries,
                "rotations": c.rotations, "refreshes": c.refreshes, "oldLocks": c.old_locks,
                "oldLockErrorsSeen": c.old_lock_errors_seen,
            },
            "responses": c.responses,
            "errorResponses": c.error_responses,
            "diagnosticsPublished": c.diagnostics_published,
        })
    }

    pub fn render(&self) -> String {
        let c = &self.counters;
        let mut out = format!(
            "soak {} after {}s\n  actions: {} edits, {} undos, {} saves, {} queries, {} file rotations, {} refreshes, {} old locks ({} {UNSUPPORTED_LOCK_CODE} seen)\n  responses: {} ({} errors), diagnostics published: {}\n",
            if self.passed() { "PASS" } else { "FAIL" },
            self.duration.as_secs(),
            c.edits,
            c.undos,
            c.saves,
            c.queries,
            c.rotations,
            c.refreshes,
            c.old_locks,
            c.old_lock_errors_seen,
            c.responses,
            c.error_responses,
            c.diagnostics_published,
        );
        match &self.memory {
            Some(m) => out.push_str(&format!(
                "  memory: first window {} KiB, last window {} KiB, growth {:.1}%{}\n",
                m.first_window_kib,
                m.last_window_kib,
                m.growth_percent,
                if self.memory_enforced { "" } else { " (not enforced in this run length)" }
            )),
            None => out.push_str("  memory: not enough samples\n"),
        }
        match self.index_finished_after {
            Some(at) => out.push_str(&format!("  initial index finished after {}s\n", at.as_secs())),
            None => out.push_str("  initial index did not finish\n"),
        }
        for (method, samples) in &self.latencies {
            if let Some(summary) = latency_summary(samples) {
                out.push_str(&format!(
                    "  latency {method}: {} requests, p50 {} ms, p95 {} ms, max {} ms{}\n",
                    summary.count,
                    summary.p50.as_millis(),
                    summary.p95.as_millis(),
                    summary.max.as_millis(),
                    if self.latency_enforced { "" } else { " (not enforced in this run)" }
                ));
            }
        }
        out.push_str(&format!(
            "  crash reports: {} notifications, {} new files\n",
            self.crash_reports.len(),
            self.new_crash_files.len()
        ));
        for failure in &self.failures {
            out.push_str(&format!("  FAILURE: {failure}\n"));
        }
        if let Some(log) = &self.server_log {
            out.push_str(&format!("  server stderr: {}\n", log.display()));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------------------------

enum Incoming {
    Message(Value),
    Closed(String),
}

/// Bounded queue in front of a writer thread, so a server that stops reading stdin makes the
/// soak fail after `drain_timeout` instead of blocking in `write` forever.
struct Outbox {
    tx: SyncSender<Vec<u8>>,
    error: Arc<Mutex<Option<String>>>,
    drain_timeout: Duration,
}

impl Outbox {
    fn new<W: Write + Send + 'static>(mut writer: W, capacity: usize, drain_timeout: Duration) -> Outbox {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(capacity);
        let error = Arc::new(Mutex::new(None));
        let failed = Arc::clone(&error);
        thread::spawn(move || {
            while let Ok(bytes) = rx.recv() {
                if let Err(e) = writer.write_all(&bytes).and_then(|_| writer.flush()) {
                    if let Ok(mut slot) = failed.lock() {
                        *slot = Some(e.to_string());
                    }
                    return;
                }
            }
        });
        Outbox { tx, error, drain_timeout }
    }

    fn send(&self, bytes: Vec<u8>) -> Result<(), String> {
        let deadline = Instant::now() + self.drain_timeout;
        let mut item = bytes;
        loop {
            if let Some(error) = self.error.lock().ok().and_then(|slot| slot.clone()) {
                return Err(format!("server stdin write failed: {error}"));
            }
            match self.tx.try_send(item) {
                Ok(()) => return Ok(()),
                Err(TrySendError::Full(back)) => {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "server stopped reading its stdin for more than {}s",
                            self.drain_timeout.as_secs()
                        ));
                    }
                    item = back;
                    thread::sleep(Duration::from_millis(10));
                }
                Err(TrySendError::Disconnected(_)) => {
                    let error = self.error.lock().ok().and_then(|slot| slot.clone()).unwrap_or_default();
                    return Err(format!("server stdin writer ended: {error}"));
                }
            }
        }
    }
}

/// Run a command to completion, killing it when it outlives `timeout`.
pub fn run_with_timeout(command: &mut Command, timeout: Duration) -> Result<ExitStatus, String> {
    let mut child = command.spawn().map_err(|e| format!("cannot start: {e}"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(e) => return Err(format!("wait failed: {e}")),
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("still running after {}s, killed", timeout.as_secs()));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct Client {
    child: Child,
    outbox: Outbox,
    diagnostics: HashMap<String, Vec<Value>>,
    rx: Receiver<Incoming>,
    next_id: i64,
    pending: HashMap<i64, (String, Instant)>,
    report: SoakReport,
    spawned: Instant,
    index_finished: Option<Instant>,
}

impl Client {
    fn spawn(config: &SoakConfig, home: &Path, log: &Path) -> io::Result<Client> {
        let mut child = Command::new(&config.server)
            .env("HOME", home)
            .env("USERPROFILE", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(File::create(log)?))
            .spawn()?;
        let stdin: ChildStdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match framing::read_message(&mut reader) {
                    Ok(Some(message)) => {
                        if tx.send(Incoming::Message(message)).is_err() {
                            return;
                        }
                    }
                    Ok(None) => {
                        let _ = tx.send(Incoming::Closed("server closed stdout".into()));
                        return;
                    }
                    Err(error) => {
                        let _ = tx.send(Incoming::Closed(format!("framing error from server: {error}")));
                        return;
                    }
                }
            }
        });
        Ok(Client {
            child,
            outbox: Outbox::new(stdin, 256, config.drain_timeout),
            diagnostics: HashMap::new(),
            rx,
            next_id: 1,
            pending: HashMap::new(),
            report: SoakReport::default(),
            spawned: Instant::now(),
            index_finished: None,
        })
    }

    fn fail(&mut self, message: String) {
        if !self.report.failures.contains(&message) {
            self.report.failures.push(message);
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        let message = json!({"jsonrpc": "2.0", "method": method, "params": params});
        if let Err(error) = self.outbox.send(framing::encode(&message)) {
            self.fail(format!("write of `{method}` failed: {error}"));
        }
    }

    fn request(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.pending.insert(id, (method.to_string(), Instant::now()));
        if let Err(error) = self.outbox.send(framing::encode(&message)) {
            self.fail(format!("write of `{method}` failed: {error}"));
        }
        id
    }

    fn handle(&mut self, message: Value) {
        if message.get("method").is_some() && message.get("id").is_some() {
            let reply = answer_server_request(&message);
            if let Err(error) = self.outbox.send(framing::encode(&reply)) {
                self.fail(format!("write of a server-request reply failed: {error}"));
            }
        } else if message.get("method").is_some() {
            if is_crash_report(&message) {
                let text = message.pointer("/params/summary").and_then(Value::as_str).unwrap_or("(no summary)");
                let path = message.pointer("/params/reportPath").and_then(Value::as_str).unwrap_or("(no path)");
                self.report.crash_reports.push(format!("{text} [{path}]"));
            } else if is_index_idle(&message) {
                if self.index_finished.is_none() {
                    let now = Instant::now();
                    self.index_finished = Some(now);
                    self.report.index_finished_after = Some(now - self.spawned);
                }
            } else if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics") {
                self.report.counters.diagnostics_published += 1;
                if let Some(uri) = message.pointer("/params/uri").and_then(Value::as_str) {
                    let list =
                        message.pointer("/params/diagnostics").and_then(Value::as_array).cloned().unwrap_or_default();
                    self.diagnostics.insert(uri.to_string(), list);
                }
            }
        } else if let Some(id) = message.get("id").and_then(Value::as_i64) {
            let (method, sent) = self.pending.remove(&id).unwrap_or_else(|| ("(unknown)".into(), Instant::now()));
            self.report.counters.responses += 1;
            if self.index_finished.is_some_and(|finished| sent >= finished) && message.get("error").is_none() {
                self.report.latencies.entry(method.clone()).or_default().push(sent.elapsed());
            }
            if message.get("error").is_some() {
                self.report.counters.error_responses += 1;
                if method == "initialize" {
                    let text = message.pointer("/error/message").and_then(Value::as_str).unwrap_or("");
                    self.fail(format!("`initialize` was answered with an error: {text}"));
                }
                if is_internal_error_response(&message) {
                    let text = message.pointer("/error/message").and_then(Value::as_str).unwrap_or("");
                    self.report.internal_errors.push(format!("{method}: {text}"));
                }
            }
        }
    }

    /// Process incoming messages for up to `wait`, then run liveness checks.
    fn pump(&mut self, wait: Duration, request_timeout: Duration) {
        let deadline = Instant::now() + wait;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(remaining) {
                Ok(Incoming::Message(message)) => self.handle(message),
                Ok(Incoming::Closed(reason)) => {
                    self.fail(format!("server connection lost: {reason}"));
                    break;
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    self.fail("reader thread ended".into());
                    break;
                }
            }
            if remaining.is_zero() {
                break;
            }
        }
        if let Ok(Some(status)) = self.child.try_wait() {
            self.fail(format!("server exited during the run: {status}"));
        }
        let stuck: Vec<String> = self
            .pending
            .values()
            .filter(|(_, since)| since.elapsed() > request_timeout)
            .map(|(method, _)| method.clone())
            .collect();
        for method in stuck {
            self.fail(format!("request `{method}` unanswered for more than {}s", request_timeout.as_secs()));
        }
    }

    /// Diagnostics with `code` last published for `uri`.
    fn diagnostics_with_code(&self, uri: &str, code: &str) -> Vec<Value> {
        self.diagnostics
            .get(uri)
            .map(|list| list.iter().filter(|d| diagnostic_code(d) == Some(code)).cloned().collect())
            .unwrap_or_default()
    }

    fn server_alive(&self) -> bool {
        self.report.failures.is_empty()
    }

    fn rss_kib(&self) -> Option<u64> {
        let output = Command::new("ps").args(["-o", "rss=", "-p", &self.child.id().to_string()]).output().ok()?;
        parse_rss_output(&String::from_utf8_lossy(&output.stdout))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The `code` of a diagnostic as a string (servers send strings; numbers are rendered).
pub fn diagnostic_code(diagnostic: &Value) -> Option<&str> {
    diagnostic.get("code").and_then(Value::as_str)
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if SKIPPED_DIRECTORIES.iter().any(|skip| name.to_string_lossy() == *skip) {
                continue;
            }
            copy_tree(&entry.path(), &to.join(&name))?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

fn collect_files(root: &Path, out_sources: &mut Vec<PathBuf>, out_locks: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else { return };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_dir() {
            collect_files(&path, out_sources, out_locks);
        } else if path.extension().is_some_and(|e| e == "bd") {
            out_sources.push(path);
        } else if path.file_name().is_some_and(|n| n == "Project.lock") {
            out_locks.push(path);
        }
    }
}

fn unique_temp_dir(label: &str) -> io::Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("beskid-soak-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        items.swap(i, rng.below(i + 1));
    }
}

fn pick_closed(sources: &[PathBuf], open: &[Document], rng: &mut Rng) -> Option<PathBuf> {
    let closed: Vec<&PathBuf> = sources.iter().filter(|p| !open.iter().any(|d| &d.path == *p)).collect();
    (!closed.is_empty()).then(|| closed[rng.below(closed.len())].clone())
}

// ---------------------------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------------------------

/// Run the soak. `Err` is a setup failure (cannot spawn, empty corpus); a soak failure is a report
/// with failures.
pub fn run(config: &SoakConfig, smoke: bool) -> Result<SoakReport, String> {
    let work = match &config.work_dir {
        Some(dir) => {
            fs::create_dir_all(dir).map_err(|e| format!("work dir: {e}"))?;
            fs::canonicalize(dir).map_err(|e| format!("work dir: {e}"))?
        }
        None => unique_temp_dir("work").map_err(|e| format!("work dir: {e}"))?,
    };
    let home = match &config.home {
        Some(dir) => {
            fs::create_dir_all(dir).map_err(|e| format!("home: {e}"))?;
            fs::canonicalize(dir).map_err(|e| format!("home: {e}"))?
        }
        None => unique_temp_dir("home").map_err(|e| format!("home: {e}"))?,
    };
    let crash_dir = home.join(".beskid").join("crash");

    // Copy the corpora so saves and old-format locks never touch the originals.
    let mut folders = Vec::new();
    for (index, corpus) in config.corpora.iter().enumerate() {
        let name = corpus.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "corpus".into());
        let target = work.join(format!("{index}-{name}"));
        copy_tree(corpus, &target).map_err(|e| format!("copy {}: {e}", corpus.display()))?;
        folders.push(target);
    }
    let mut sources = Vec::new();
    let mut locks = Vec::new();
    for folder in &folders {
        collect_files(folder, &mut sources, &mut locks);
    }
    if sources.is_empty() {
        return Err("the corpora contain no .bd files".into());
    }
    let mut rng = Rng::new(config.seed);
    shuffle(&mut sources, &mut rng);

    let crash_before = list_directory(&crash_dir);
    let log = work.join("beskid_lsp.stderr.log");
    let mut client =
        Client::spawn(config, &home, &log).map_err(|e| format!("cannot start {}: {e}", config.server.display()))?;
    client.report.server_log = Some(log);

    // Handshake.
    let workspace_folders: Vec<Value> = folders
        .iter()
        .map(|f| json!({"uri": path_to_uri(f), "name": f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()}))
        .collect();
    let init_id = client.request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "clientInfo": {"name": "beskid-soak"},
            "rootUri": path_to_uri(&folders[0]),
            "workspaceFolders": workspace_folders,
            "capabilities": {
                "workspace": {"workspaceFolders": true, "configuration": true,
                              "didChangeWatchedFiles": {"dynamicRegistration": false}},
                "textDocument": {"synchronization": {"didSave": true},
                                 "publishDiagnostics": {}}
            }
        }),
    );
    let handshake_deadline = Instant::now() + Duration::from_secs(120);
    while client.pending.contains_key(&init_id) && client.server_alive() {
        if Instant::now() > handshake_deadline {
            client.fail("server did not answer `initialize` within 120s".into());
            break;
        }
        client.pump(Duration::from_millis(200), config.request_timeout);
    }
    if client.server_alive() {
        client.notify("initialized", json!({}));
    }

    let started = Instant::now();
    let mut open: Vec<Document> = Vec::new();
    let mut cursor = 0;
    while open.len() < config.max_open && cursor < sources.len() {
        let path = sources[cursor].clone();
        cursor += 1;
        if let Ok(text) = fs::read_to_string(&path) {
            let doc = Document::new(path, text);
            client.notify("textDocument/didOpen", did_open(&doc));
            open.push(doc);
        }
    }

    let mut next_sample = Duration::ZERO;
    let old_lock_interval = (config.duration / 4).max(Duration::from_secs(15));
    let mut next_old_lock = old_lock_interval.min(config.duration / 2);
    let mut old_locks_done = 0usize;

    while started.elapsed() < config.duration && client.server_alive() {
        let elapsed = started.elapsed();
        if elapsed >= next_sample {
            if let Some(kib) = client.rss_kib() {
                client.report.samples.push(RssSample { at: elapsed, kib });
            }
            next_sample = elapsed + config.sample_interval;
        }
        if elapsed >= next_old_lock && !locks.is_empty() {
            let lock = locks[old_locks_done % locks.len()].clone();
            old_locks_done += 1;
            next_old_lock = elapsed + old_lock_interval;
            old_lock_fails_closed(&mut client, config, &lock, &mut open);
        } else {
            step(&mut client, &mut rng, &mut open, &sources, &folders);
        }
        client.pump(config.tick, config.request_timeout);
    }
    let ran_for = started.elapsed();
    if let Some(kib) = client.rss_kib() {
        client.report.samples.push(RssSample { at: ran_for, kib });
    }

    // Orderly shutdown; a server that is already gone has been reported.
    if client.server_alive() {
        let id = client.request("shutdown", Value::Null);
        let deadline = Instant::now() + Duration::from_secs(30);
        while client.pending.contains_key(&id) && Instant::now() < deadline && client.server_alive() {
            client.pump(Duration::from_millis(200), config.request_timeout);
        }
        client.notify("exit", Value::Null);
        let wait_until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < wait_until {
            if matches!(client.child.try_wait(), Ok(Some(_))) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    let _ = client.child.kill();
    let _ = client.child.wait();

    // Verdicts.
    let mut report = std::mem::take(&mut client.report);
    report.duration = ran_for;
    let crash_after = list_directory(&crash_dir);
    report.new_crash_files = new_entries(&crash_before, &crash_after);
    if !report.crash_reports.is_empty() {
        report
            .failures
            .push(format!("{} `{CRASH_REPORT_METHOD}` notification(s) received", report.crash_reports.len()));
    }
    if !report.new_crash_files.is_empty() {
        report.failures.push(format!(
            "new crash file(s) in {}: {}",
            crash_dir.display(),
            report.new_crash_files.join(", ")
        ));
    }
    if !report.internal_errors.is_empty() {
        report.failures.push(format!(
            "{} request(s) answered with an internal error, first: {}",
            report.internal_errors.len(),
            report.internal_errors[0]
        ));
    }
    if let Some(failure) = old_lock_probe_failure(&report.counters) {
        report.failures.push(failure);
    }
    let window = Duration::from_secs(60).min(ran_for / 3);
    report.memory = memory_growth(&report.samples, ran_for, window);
    // First and last window must be distinct and meaningful: enforce only on full-length runs.
    report.memory_enforced = !smoke && ran_for >= Duration::from_secs(180);
    if let Some(m) = &report.memory
        && report.memory_enforced
        && m.growth_percent >= config.memory_growth_limit_percent
    {
        report.failures.push(format!(
            "memory grew {:.1}% (limit {:.0}%): {} KiB to {} KiB",
            m.growth_percent, config.memory_growth_limit_percent, m.first_window_kib, m.last_window_kib
        ));
    }
    if report.memory.is_none() && report.memory_enforced {
        report.failures.push("no RSS samples: cannot judge memory growth (is `ps` available?)".into());
    }
    // The initial index of a large corpus can outlast a smoke run; budgets hold on full runs.
    report.latency_enforced = !smoke;
    if report.latency_enforced {
        let index_finished = report.index_finished_after.is_some();
        let failures = latency_failures(&config.latency_budgets, &report.latencies, index_finished);
        report.failures.extend(failures);
    }
    Ok(report)
}

fn did_open(doc: &Document) -> Value {
    json!({"textDocument": {"uri": doc.uri, "languageId": "beskid", "version": doc.version, "text": doc.text}})
}

fn did_change(doc: &Document) -> Value {
    json!({"textDocument": {"uri": doc.uri, "version": doc.version}, "contentChanges": [{"text": doc.text}]})
}

fn text_position(doc: &Document, rng: &mut Rng) -> Value {
    let (line, character) = doc.random_position(rng);
    json!({"textDocument": {"uri": doc.uri}, "position": {"line": line, "character": character}})
}

/// One random action.
fn step(client: &mut Client, rng: &mut Rng, open: &mut Vec<Document>, sources: &[PathBuf], folders: &[PathBuf]) {
    if open.is_empty() {
        return;
    }
    let index = rng.below(open.len());
    match rng.below(100) {
        0..=34 => {
            if open[index].random_edit(rng) {
                client.report.counters.edits += 1;
                client.notify("textDocument/didChange", did_change(&open[index]));
            }
        }
        35..=44 => {
            if open[index].undo() {
                client.report.counters.undos += 1;
                client.notify("textDocument/didChange", did_change(&open[index]));
            }
        }
        45..=54 => {
            let doc = &open[index];
            if fs::write(&doc.path, &doc.text).is_ok() {
                client.report.counters.saves += 1;
                client.notify("textDocument/didSave", json!({"textDocument": {"uri": doc.uri}, "text": doc.text}));
            }
        }
        55..=79 => {
            client.report.counters.queries += 1;
            let doc = open[index].clone();
            match rng.below(6) {
                0 => {
                    client.request("textDocument/hover", text_position(&doc, rng));
                }
                1 => {
                    client.request("textDocument/definition", text_position(&doc, rng));
                }
                2 => {
                    client.request("textDocument/completion", text_position(&doc, rng));
                }
                3 => {
                    client.request("textDocument/documentSymbol", json!({"textDocument": {"uri": doc.uri}}));
                }
                4 => {
                    client.request("textDocument/semanticTokens/full", json!({"textDocument": {"uri": doc.uri}}));
                }
                _ => {
                    client.request(
                        "textDocument/formatting",
                        json!({
                            "textDocument": {"uri": doc.uri},
                            "options": {"tabSize": 4, "insertSpaces": true}
                        }),
                    );
                }
            }
        }
        80..=89 => {
            // Rotate: close one document, open another from the corpus.
            if let Some(path) = pick_closed(sources, open, rng)
                && let Ok(text) = fs::read_to_string(&path)
            {
                let closed = open.remove(index);
                client.notify("textDocument/didClose", json!({"textDocument": {"uri": closed.uri}}));
                let doc = Document::new(path, text);
                client.notify("textDocument/didOpen", did_open(&doc));
                open.push(doc);
                client.report.counters.rotations += 1;
            }
        }
        _ => {
            // Workspace refresh: file watcher event for a file and a workspace-wide symbol search.
            client.report.counters.refreshes += 1;
            let doc = &open[index];
            client.notify("workspace/didChangeWatchedFiles", json!({"changes": [{"uri": doc.uri, "type": 2}]}));
            if rng.chance(50) {
                client.request("workspace/symbol", json!({"query": ""}));
            } else {
                let folder = &folders[rng.below(folders.len())];
                client.notify("workspace/didChangeWorkspaceFolders", json!({"event": {"added": [], "removed": []}}));
                client.notify(
                    "workspace/didChangeWatchedFiles",
                    json!({"changes": [{"uri": path_to_uri(folder), "type": 2}]}),
                );
            }
        }
    }
}

/// An old-format lock fails closed: downgrade the lock header, tell the server and wait for the
/// `E3025` diagnostic it publishes on the project manifest. With the CLI, `beskid dev project lock`
/// must refuse the lock and leave its bytes; then the lock is deleted, as the message tells the
/// user, and `beskid dev project lock` must write the current format again.
fn old_lock_fails_closed(client: &mut Client, config: &SoakConfig, lock: &Path, open: &mut [Document]) {
    let Ok(original) = fs::read_to_string(lock) else { return };
    let Some(old) = downgrade_lock_to_v1(&original) else { return };
    if fs::write(lock, &old).is_err() {
        return;
    }
    client.report.counters.old_locks += 1;
    let lock_uri = path_to_uri(lock);
    client.notify("workspace/didChangeWatchedFiles", json!({"changes": [{"uri": lock_uri, "type": 2}]}));
    client.pump(Duration::from_secs(2), config.request_timeout);

    // The manifest next to the lock reports the error.
    if let Some(dir) = lock.parent()
        && let Ok(entries) = fs::read_dir(dir)
        && let Some(manifest) =
            entries.filter_map(Result::ok).map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "bproj"))
        && let Ok(text) = fs::read_to_string(&manifest)
    {
        let uri = path_to_uri(&manifest);
        client.diagnostics.remove(&uri);
        client.notify(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": "beskid-project", "version": 1, "text": text}}),
        );
        let deadline = Instant::now() + OLD_LOCK_DIAGNOSTIC_WAIT;
        while client.diagnostics_with_code(&uri, UNSUPPORTED_LOCK_CODE).is_empty()
            && Instant::now() < deadline
            && client.server_alive()
        {
            client.pump(Duration::from_millis(200), config.request_timeout);
        }
        if !client.diagnostics_with_code(&uri, UNSUPPORTED_LOCK_CODE).is_empty() {
            client.report.counters.old_lock_errors_seen += 1;
        }
        client.notify("textDocument/didClose", json!({"textDocument": {"uri": uri}}));
    }

    if let (Some(cli), Some(dir)) = (&config.beskid_cli, lock.parent()) {
        let lock_command = || {
            let mut command = Command::new(cli);
            command
                .args(["dev", "project", "lock", "--plain", "--project"])
                .arg(dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            command
        };
        // The old lock is refused and its bytes stay.
        match run_with_timeout(&mut lock_command(), config.cli_timeout) {
            Ok(status) if status.success() => {
                client.fail(format!("`beskid dev project lock` accepted the old-format lock {}", lock.display()))
            }
            Ok(_) => {
                if fs::read_to_string(lock).ok().as_deref() != Some(old.as_str()) {
                    client.fail(format!("`beskid dev project lock` changed the refused lock {}", lock.display()));
                }
            }
            Err(error) => client.fail(format!("`beskid dev project lock` for {}: {error}", dir.display())),
        }
        // Deleted, the lock is written again in the current format.
        let _ = fs::remove_file(lock);
        match run_with_timeout(&mut lock_command(), config.cli_timeout) {
            Ok(status) if status.success() => {
                let written = fs::read_to_string(lock).unwrap_or_default();
                if !written.starts_with(CURRENT_LOCK_HEADER) {
                    client.fail(format!(
                        "`beskid dev project lock` succeeded but {} does not start with `{}`",
                        lock.display(),
                        CURRENT_LOCK_HEADER.trim_end()
                    ));
                }
            }
            Ok(status) => client.fail(format!(
                "`beskid dev project lock` for {} failed after the old lock was deleted: {status}",
                dir.display()
            )),
            Err(error) => client.fail(format!("`beskid dev project lock` for {}: {error}", dir.display())),
        }
        if !lock.exists() {
            let _ = fs::write(lock, &original);
        }
    } else {
        let _ = fs::write(lock, &original);
    }
    client.notify("workspace/didChangeWatchedFiles", json!({"changes": [{"uri": lock_uri, "type": 2}]}));
    // Keep documents that live next to the lock in sync with the server's view.
    for doc in open.iter() {
        if lock.parent().is_some_and(|dir| doc.path.starts_with(dir)) {
            client.notify("textDocument/didSave", json!({"textDocument": {"uri": doc.uri}, "text": doc.text}));
        }
    }
}

/// Verdict on the old-lock probe: a run that wrote old-format locks must have seen the server
/// report `E3025` for at least one of them.
pub fn old_lock_probe_failure(counters: &Counters) -> Option<String> {
    (counters.old_locks > 0 && counters.old_lock_errors_seen == 0).then(|| {
        format!(
            "{} old-format lock(s) written but the server never published an `{UNSUPPORTED_LOCK_CODE}` diagnostic on the manifest",
            counters.old_locks
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(secs: u64, kib: u64) -> RssSample {
        RssSample { at: Duration::from_secs(secs), kib }
    }

    #[test]
    fn latency_budgets_parse_and_replace_the_default_for_their_method() {
        let budget = parse_latency_budget("textDocument/hover=250ms,2s").unwrap();
        assert_eq!(budget.p95, Duration::from_millis(250));
        assert_eq!(budget.max, Duration::from_secs(2));
        assert!(parse_latency_budget("textDocument/hover=2s,250ms").is_err(), "p95 above max");
        assert!(parse_latency_budget("textDocument/hover=250ms").is_err());
        assert!(parse_latency_budget("=1s,2s").is_err());
        let to_args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (config, _) =
            SoakConfig::from_args(to_args(&["--corpus", "/a", "--latency-budget", "textDocument/completion=1s,3s"]))
                .unwrap();
        let completion: Vec<_> =
            config.latency_budgets.iter().filter(|b| b.method == "textDocument/completion").collect();
        assert_eq!(completion.len(), 1);
        assert_eq!(completion[0].p95, Duration::from_secs(1));
        assert_eq!(config.latency_budgets.len(), default_latency_budgets().len());
    }

    #[test]
    fn latency_summary_uses_nearest_rank() {
        let samples: Vec<Duration> = (1..=20).map(Duration::from_millis).collect();
        let summary = latency_summary(&samples).unwrap();
        assert_eq!(
            (summary.count, summary.p50, summary.p95, summary.max),
            (20, Duration::from_millis(10), Duration::from_millis(19), Duration::from_millis(20))
        );
        assert!(latency_summary(&[]).is_none());
    }

    #[test]
    fn latency_verdict_fails_slow_tails_and_unmeasured_methods() {
        let budgets = vec![
            LatencyBudget { method: "a".into(), p95: Duration::from_millis(500), max: Duration::from_secs(5) },
            LatencyBudget { method: "b".into(), p95: Duration::from_millis(500), max: Duration::from_secs(5) },
        ];
        let mut measured = BTreeMap::new();
        measured.insert("a".to_string(), vec![Duration::from_millis(10); 40]);
        measured.insert("b".to_string(), vec![Duration::from_millis(10); 40]);
        assert!(latency_failures(&budgets, &measured, true).is_empty());

        measured.get_mut("a").unwrap().extend([Duration::from_secs(6), Duration::from_millis(600)]);
        measured.get_mut("b").unwrap().extend(vec![Duration::from_millis(900); 10]);
        let failures = latency_failures(&budgets, &measured, true);
        assert_eq!(failures.len(), 2, "{failures:?}");
        assert!(failures[0].contains("`a` took 6000 ms"), "{failures:?}");
        assert!(failures[1].contains("`b` p95 900 ms"), "{failures:?}");

        measured.remove("b");
        let unfinished = latency_failures(&budgets, &measured, false);
        assert!(unfinished.iter().any(|failure| failure.contains("`b` latency was not measured")), "{unfinished:?}");
    }

    #[test]
    fn durations_parse_with_units() {
        assert_eq!(parse_duration("250ms").unwrap(), Duration::from_millis(250));
        assert_eq!(parse_duration("90").unwrap(), Duration::from_secs(90));
        assert_eq!(parse_duration("90s").unwrap(), Duration::from_secs(90));
        assert_eq!(parse_duration("30m").unwrap(), Duration::from_secs(1800));
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
        assert!(parse_duration("m").is_err());
        assert!(parse_duration("5d").is_err());
    }

    #[test]
    fn args_require_a_corpus_and_smoke_forces_sixty_seconds() {
        let to_args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(SoakConfig::from_args(to_args(&["--duration", "5m"])).is_err());
        let (config, smoke) =
            SoakConfig::from_args(to_args(&["--corpus", "/a", "--corpus", "/b", "--duration", "5m", "--smoke"]))
                .unwrap();
        assert!(smoke);
        assert_eq!(config.duration, Duration::from_secs(60));
        assert_eq!(config.corpora.len(), 2);
        let (config, smoke) = SoakConfig::from_args(to_args(&["--corpus", "/a"])).unwrap();
        assert!(!smoke);
        assert_eq!(config.duration, Duration::from_secs(1800));
        assert!(SoakConfig::from_args(to_args(&["--corpus", "/a", "--bogus"])).is_err());
        assert!(SoakConfig::from_args(to_args(&["--corpus"])).is_err());
    }

    #[test]
    fn memory_growth_compares_first_and_last_window() {
        // The first window skips the warm-up: samples at or before the window length do not count.
        let samples = [
            sample(10, 10_000),
            sample(50, 20_000),
            sample(70, 100_000),
            sample(110, 100_000),
            sample(500, 110_000),
            sample(590, 114_000),
            sample(600, 114_000),
        ];
        let verdict = memory_growth(&samples, Duration::from_secs(600), Duration::from_secs(60)).unwrap();
        assert_eq!(verdict.first_window_kib, 100_000);
        assert_eq!(verdict.last_window_kib, 114_000);
        assert!((verdict.growth_percent - 14.0).abs() < 0.001);
    }

    #[test]
    fn memory_growth_detects_a_leak_and_handles_missing_windows() {
        let leak = [sample(5, 100_000), sample(90, 100_000), sample(1195, 150_000), sample(1200, 150_000)];
        let verdict = memory_growth(&leak, Duration::from_secs(1200), Duration::from_secs(60)).unwrap();
        assert!(verdict.growth_percent > 20.0);
        let no_last = [sample(90, 100_000)];
        let only_warmup = [sample(5, 100_000), sample(1195, 150_000)];
        assert!(memory_growth(&only_warmup, Duration::from_secs(1200), Duration::from_secs(60)).is_none());
        assert!(memory_growth(&no_last, Duration::from_secs(1200), Duration::from_secs(60)).is_none());
        assert!(memory_growth(&[], Duration::from_secs(60), Duration::from_secs(20)).is_none());
    }

    #[test]
    fn old_lock_probe_fails_when_the_error_is_never_published() {
        let mut counters = Counters::default();
        assert!(old_lock_probe_failure(&counters).is_none(), "no old lock, no probe");
        counters.old_locks = 2;
        assert!(old_lock_probe_failure(&counters).unwrap().contains("never published an `E3025`"));
        counters.old_lock_errors_seen = 1;
        assert!(old_lock_probe_failure(&counters).is_none());
    }

    #[test]
    fn diagnostic_codes_are_read_as_strings() {
        assert_eq!(diagnostic_code(&json!({"code": "E3025"})), Some("E3025"));
        assert_eq!(diagnostic_code(&json!({"code": 3024})), None);
    }

    struct Blocked;
    impl Write for Blocked {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            thread::sleep(Duration::from_secs(30));
            Ok(0)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_server_that_never_reads_fails_the_send_instead_of_hanging() {
        let outbox = Outbox::new(Blocked, 2, Duration::from_millis(200));
        let started = Instant::now();
        let mut result = Ok(());
        for _ in 0..10 {
            result = outbox.send(vec![0u8; 8]);
            if result.is_err() {
                break;
            }
        }
        assert!(result.unwrap_err().contains("stopped reading"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_broken_pipe_is_reported() {
        let outbox = Outbox::new(Broken, 4, Duration::from_secs(5));
        let mut result = outbox.send(vec![1]);
        for _ in 0..50 {
            if result.is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
            result = outbox.send(vec![1]);
        }
        assert!(result.unwrap_err().contains("gone"));
    }

    #[cfg(unix)]
    #[test]
    fn cli_runs_are_bounded_by_the_timeout() {
        let mut slow = Command::new("sleep");
        slow.arg("30");
        let started = Instant::now();
        let error = run_with_timeout(&mut slow, Duration::from_millis(300)).unwrap_err();
        assert!(error.contains("killed"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10));
        let mut quick = Command::new("true");
        assert!(run_with_timeout(&mut quick, Duration::from_secs(10)).unwrap().success());
        let mut failing = Command::new("false");
        assert!(!run_with_timeout(&mut failing, Duration::from_secs(10)).unwrap().success());
    }

    #[test]
    fn rss_output_parses_ps_lines() {
        assert_eq!(parse_rss_output("  123456\n"), Some(123_456));
        assert_eq!(parse_rss_output(""), None);
        assert_eq!(parse_rss_output("RSS"), None);
    }

    #[test]
    fn new_crash_files_are_the_difference() {
        let before: BTreeSet<String> = ["lsp-1.txt".to_string()].into();
        let after: BTreeSet<String> = ["lsp-1.txt".to_string(), "lsp-2.txt".to_string()].into();
        assert_eq!(new_entries(&before, &after), vec!["lsp-2.txt".to_string()]);
        assert!(new_entries(&after, &after).is_empty());
    }

    #[test]
    fn uris_percent_encode_unsafe_bytes() {
        assert_eq!(path_to_uri(Path::new("/tmp/a b/\u{105}.bd")), "file:///tmp/a%20b/%C4%85.bd");
    }

    #[test]
    fn lock_downgrade_keeps_the_project_and_drops_entries() {
        let current = "# Project.lock v3\nroot_manifest=x.bproj\nproject_name=x\ndependencies:\n- name=Core;source=corelib;project=beskid_corelib;manifest=corelib.bproj;source_root=.\n";
        let v1 = downgrade_lock_to_v1(current).unwrap();
        assert_eq!(v1, "# Project.lock v1\nroot_manifest=x.bproj\nproject_name=x\ndependencies:\n");
        assert_eq!(downgrade_lock_to_v1(&v1), None);
        assert_eq!(downgrade_lock_to_v1("nonsense"), None);
    }

    #[test]
    fn server_requests_get_neutral_replies_with_matching_id() {
        let configuration = json!({"jsonrpc": "2.0", "id": 7, "method": "workspace/configuration",
                                   "params": {"items": [{"section": "a"}, {"section": "b"}]}});
        assert_eq!(answer_server_request(&configuration), json!({"jsonrpc": "2.0", "id": 7, "result": [null, null]}));
        let register = json!({"jsonrpc": "2.0", "id": "x", "method": "client/registerCapability", "params": {}});
        assert_eq!(answer_server_request(&register), json!({"jsonrpc": "2.0", "id": "x", "result": null}));
    }

    #[test]
    fn crash_report_and_internal_error_detection() {
        assert!(is_crash_report(&json!({"jsonrpc": "2.0", "method": "beskid/crashReport", "params": {}})));
        assert!(!is_crash_report(&json!({"jsonrpc": "2.0", "id": 1, "method": "beskid/crashReport"})));
        assert!(!is_crash_report(&json!({"jsonrpc": "2.0", "method": "window/logMessage"})));
        assert!(is_internal_error_response(&json!({"id": 3, "error": {"code": -32603, "message": "internal error"}})));
        assert!(!is_internal_error_response(&json!({"id": 3, "error": {"code": -32601, "message": "x"}})));
        assert!(!is_internal_error_response(&json!({"id": 3, "result": null})));
    }

    #[test]
    fn document_edit_and_undo_restore_the_text() {
        let original = "i32 Main() {\n    return 1;\n}\n".to_string();
        let mut doc = Document::new(PathBuf::from("/tmp/Main.bd"), original.clone());
        let mut rng = Rng::new(5);
        let mut edits = 0;
        for _ in 0..20 {
            if doc.random_edit(&mut rng) {
                edits += 1;
            }
        }
        assert!(edits > 0);
        assert_eq!(doc.version, 1 + edits);
        while doc.undo() {}
        assert_eq!(doc.text, original);
        assert!(!doc.undo());
    }

    #[test]
    fn undo_history_is_bounded() {
        let mut doc = Document::new(PathBuf::from("/tmp/Main.bd"), "a b c d e f g h".into());
        let mut rng = Rng::new(2);
        for _ in 0..500 {
            doc.random_edit(&mut rng);
        }
        assert!(doc.history.len() <= HISTORY_LIMIT);
    }

    #[test]
    fn random_position_exists_in_the_text() {
        let doc = Document::new(PathBuf::from("/tmp/Main.bd"), "ab\n\u{105}\u{107}\n".into());
        let mut rng = Rng::new(1);
        for _ in 0..200 {
            let (line, character) = doc.random_position(&mut rng);
            let lines: Vec<&str> = doc.text.split('\n').collect();
            assert!((line as usize) < lines.len());
            assert!(character as usize <= lines[line as usize].encode_utf16().count());
        }
    }

    #[test]
    fn report_fails_when_any_failure_is_recorded() {
        let mut report = SoakReport::default();
        assert!(report.passed());
        report.failures.push("server exited".into());
        assert!(!report.passed());
        assert_eq!(report.to_json()["result"], "fail");
        assert!(report.render().contains("FAILURE: server exited"));
    }
}
