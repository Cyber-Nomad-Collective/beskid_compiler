//! `beskid fmt` — canonical pretty-printer for `.bd` sources (file, tree, or check).

use anyhow::{Context, Result, bail};
use beskid_analysis::MietteReportError;
use beskid_analysis::format::format_source;
use clap::Args;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

#[derive(Args, Debug)]
pub struct FormatArgs {
    #[command(flatten)]
    pub diagnostics: crate::cli::error_format::ErrorFormatArgs,

    /// Beskid source file (`.bd`) or directory to scan recursively for `.bd` files
    #[arg(default_value = ".")]
    pub input: PathBuf,

    /// Write formatted output for a single input file to this path instead of the input
    #[arg(short = 'o', long, conflicts_with = "check")]
    pub output: Option<PathBuf>,

    /// Fail if any file would change when formatted (for CI)
    #[arg(long, conflicts_with = "output")]
    pub check: bool,
}

/// Format in place by default; check or write a separate output when requested.
///
/// Every file goes through [`format_source`], the same function the language server uses. A
/// file that does not parse is reported and left unchanged, and the command then exits non-zero.
pub fn execute(args: FormatArgs) -> Result<()> {
    let started = Instant::now();
    let input_is_dir = fs::metadata(&args.input)
        .with_context(|| format!("stat {}", args.input.display()))?
        .is_dir();

    let paths = collect_bd_files(&args.input, input_is_dir)
        .with_context(|| format!("scan {}", args.input.display()))?;

    if paths.is_empty() {
        Progress(format!(
            "beskid fmt: 0 .bd file(s) in {}",
            fmt_duration(started.elapsed())
        ));
        return Ok(());
    }

    if paths.len() > 1 && args.output.is_some() {
        bail!("`--output` is only valid when formatting a single .bd file");
    }

    // Single file → `--output`
    if paths.len() == 1
        && let Some(out) = args.output.as_ref()
    {
        let source = fs::read_to_string(&paths[0]).with_context(|| format!("read {}", paths[0].display()))?;
        let formatted = format_file(&paths[0], &source)?;
        fs::write(out, formatted).with_context(|| format!("write {}", out.display()))?;
        Progress(format!("beskid fmt: 1 file in {}", fmt_duration(started.elapsed())));
        return Ok(());
    }

    // Format every file, then fail once with the first error after reporting the others.
    let mut failures: Vec<anyhow::Error> = Vec::new();
    for path in &paths {
        if let Err(error) = format_one(path, args.check) {
            failures.push(error);
        }
    }

    let elapsed = started.elapsed();
    let label = if args.check { "checked" } else { "formatted" };
    if failures.is_empty() {
        Progress(format!("beskid fmt: {label} {} .bd file(s) in {}", paths.len(), fmt_duration(elapsed)));
        return Ok(());
    }
    let count = failures.len();
    let mut failures = failures.into_iter();
    let first = failures.next().expect("at least one failure");
    for other in failures {
        beskid_tools::print_report(&beskid_tools::diagnostics::report_from_anyhow(&other));
    }
    Progress(format!(
        "beskid fmt: {count} of {} .bd file(s) failed in {}",
        paths.len(),
        fmt_duration(elapsed)
    ));
    Err(first)
}

/// Format one file's text, or report why it cannot be formatted (the file is left unchanged).
fn format_file(path: &Path, source: &str) -> Result<String> {
    format_source(&path.display().to_string(), source)
        .map_err(|error| anyhow::Error::new(MietteReportError::new(error.into_diagnostic())))
}

fn format_one(path: &Path, check: bool) -> Result<()> {
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let formatted = format_file(path, &source)?;

    if check {
        if formatted != source {
            bail!(
                "not formatted: {} (run `beskid fmt {}`)",
                path.display(),
                path.display()
            );
        }
        return Ok(());
    }

    if formatted != source {
        fs::write(path, formatted).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

fn collect_bd_files(path: &Path, input_is_dir: bool) -> Result<Vec<PathBuf>> {
    if input_is_dir {
        let mut paths = Vec::new();
        for entry in WalkDir::new(path).into_iter().filter_entry(|e| {
            if !e.file_type().is_dir() {
                return true;
            }
            if e.depth() == 0 {
                return true;
            }
            !is_skipped_dir(e.file_name())
        }) {
            let entry = entry.map_err(|e| anyhow::anyhow!("walk {}: {e}", path.display()))?;
            if entry.file_type().is_file() && is_bd_path(entry.path()) {
                paths.push(entry.path().to_path_buf());
            }
        }
        paths.sort();
        Ok(paths)
    } else if is_bd_path(path) {
        Ok(vec![path.to_path_buf()])
    } else {
        bail!("expected a `.bd` file or directory, got {}", path.display());
    }
}

fn is_bd_path(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bd"))
}

fn is_skipped_dir(name: &OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(
            ".git"
                | ".svn"
                | ".hg"
                | "target"
                | "node_modules"
                | "dist"
                | ".venv"
                | "vendor"
                | "__pycache__"
        )
    )
}

/// Human-friendly duration: ns / µs / ms / s depending on magnitude.
fn fmt_duration(d: std::time::Duration) -> String {
    let ns = d.as_nanos();
    if ns < 1_000 {
        format!("{ns} ns")
    } else if ns < 1_000_000 {
        format!("{:.2} µs", ns as f64 / 1_000.0)
    } else if ns < 1_000_000_000 {
        format!("{:.2} ms", ns as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", ns as f64 / 1_000_000_000.0)
    }
}

#[cfg(test)]
mod v06_tests {
    use super::*;
    #[test]
    fn v06_fmt_without_write_flag_formats_selected_file() {
        let root = std::env::temp_dir().join(format!(
            "beskid-v06-fmt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("Main.bd");
        let source = "i32 Main(){return 0;}\n";
        std::fs::write(&path, source).unwrap();
        let result = execute(FormatArgs {
            input: path.clone(),
            output: None,
            check: false,
            diagnostics: Default::default(),
        });
        let actual = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        result.unwrap();
        assert_ne!(
            actual, source,
            "canonical fmt should edit the selected source without extra --write"
        );
        assert!(actual.contains("    return 0;"));
    }

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "beskid-v067-fmt-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn v067_fmt_fails_on_a_parse_error_and_leaves_the_file_unchanged() {
        let root = temp_root("broken");
        let path = root.join("Broken.bd");
        let source = "i32 Main()\n{\n    let x: i32 = 5\n    return x;\n}\n";
        std::fs::write(&path, source).unwrap();
        let result = execute(FormatArgs { input: path.clone(), output: None, check: false });
        let actual = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(result.is_err(), "a parse error must fail `beskid fmt`");
        assert_eq!(actual, source, "a file that does not parse must not be rewritten");
    }

    #[test]
    fn v067_fmt_check_passes_on_its_own_output_and_keeps_comments() {
        let root = temp_root("check");
        let path = root.join("Main.bd");
        let source = "// Header comment.\n// TODO: keep me.\n\ni32 Answer() { return 0; } // trailing\n";
        std::fs::write(&path, source).unwrap();
        execute(FormatArgs { input: path.clone(), output: None, check: false }).unwrap();
        let formatted = std::fs::read_to_string(&path).unwrap();
        let check = execute(FormatArgs { input: path.clone(), output: None, check: true });
        std::fs::remove_dir_all(root).unwrap();
        assert!(formatted.starts_with("// Header comment.\n// TODO: keep me.\n\n"), "{formatted}");
        assert!(formatted.contains("} // trailing"), "{formatted}");
        check.expect("`--check` must pass on the formatter's own output");
    }
}

/// Summary line on stderr, suppressed by `--quiet`.
#[allow(non_snake_case)]
fn Progress(line: String) {
    if beskid_telemetry::verbosity() != beskid_telemetry::Verbosity::Quiet {
        eprintln!("{line}");
    }
}
