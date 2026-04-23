use anyhow::{Context, Result, bail};
use beskid_analysis::format::format_program;
use beskid_analysis::services;
use clap::Args;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

#[derive(Args, Debug)]
pub struct FormatArgs {
    /// Beskid source file (`.bd`) or directory to scan recursively for `.bd` files
    #[arg(required = true)]
    pub input: PathBuf,

    /// Overwrite each input file with formatted output
    #[arg(short = 'w', long, conflicts_with_all = ["output", "check"])]
    pub write: bool,

    /// Write formatted output for a single input file to this path instead of stdout
    #[arg(short = 'o', long, conflicts_with_all = ["write", "check"])]
    pub output: Option<PathBuf>,

    /// Fail if any file would change when formatted (for CI)
    #[arg(long, conflicts_with_all = ["write", "output"])]
    pub check: bool,
}

pub fn execute(args: FormatArgs) -> Result<()> {
    let started = Instant::now();
    let input_is_dir = fs::metadata(&args.input)
        .with_context(|| format!("stat {}", args.input.display()))?
        .is_dir();

    let paths = collect_bd_files(&args.input, input_is_dir)
        .with_context(|| format!("scan {}", args.input.display()))?;

    if paths.is_empty() {
        eprintln!(
            "beskid format: 0 .bd file(s) in {}",
            fmt_duration(started.elapsed())
        );
        return Ok(());
    }

    if input_is_dir && !args.write && !args.check {
        bail!(
            "`{}` is a directory: use `--write` to format in place or `--check` to verify formatting",
            args.input.display()
        );
    }

    if paths.len() > 1 && args.output.is_some() {
        bail!("`--output` is only valid when formatting a single .bd file");
    }

    // Single file → stdout (no flags)
    if paths.len() == 1 && !input_is_dir && !args.write && !args.check && args.output.is_none() {
        let formatted = format_path_to_string(&paths[0])?;
        print!("{formatted}");
        eprintln!(
            "beskid format: 1 file in {}",
            fmt_duration(started.elapsed())
        );
        return Ok(());
    }

    // Single file → `--output`
    if paths.len() == 1 && let Some(out) = args.output.as_ref() {
        let formatted = format_path_to_string(&paths[0])?;
        fs::write(out, formatted).with_context(|| format!("write {}", out.display()))?;
        eprintln!(
            "beskid format: 1 file in {}",
            fmt_duration(started.elapsed())
        );
        return Ok(());
    }

    for path in &paths {
        format_one_write_or_check(path, args.write, args.check)
            .with_context(|| format!("{}", path.display()))?;
    }

    let elapsed = started.elapsed();
    let label = if args.check {
        "checked"
    } else {
        "formatted"
    };
    eprintln!(
        "beskid format: {label} {} .bd file(s) in {}",
        paths.len(),
        fmt_duration(elapsed)
    );

    Ok(())
}

fn format_path_to_string(path: &Path) -> Result<String> {
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let program = services::parse_program_with_source_name(&path.display().to_string(), &source)
        .with_context(|| format!("parse {}", path.display()))?;
    format_program(&program).map_err(|e| anyhow::anyhow!("format: {e:?}"))
}

fn format_one_write_or_check(path: &Path, write: bool, check: bool) -> Result<()> {
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let program = services::parse_program_with_source_name(&path.display().to_string(), &source)
        .with_context(|| format!("parse {}", path.display()))?;
    let formatted = format_program(&program).map_err(|e| anyhow::anyhow!("format: {e:?}"))?;

    if check {
        if formatted != source {
            bail!(
                "not formatted: {} (run `beskid format --write {}`)",
                path.display(),
                path.display()
            );
        }
        return Ok(());
    }

    if write {
        fs::write(path, formatted).with_context(|| format!("write {}", path.display()))?;
        return Ok(());
    }

    unreachable!("format_one_write_or_check: expected --write or --check")
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
        bail!(
            "expected a `.bd` file or directory, got {}",
            path.display()
        );
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
            ".git" | ".svn" | ".hg" | "target" | "node_modules" | "dist" | ".venv" | "vendor"
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
