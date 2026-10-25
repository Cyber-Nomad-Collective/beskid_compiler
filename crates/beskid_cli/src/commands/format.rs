//! `beskid fmt` — canonical pretty-printer for `.bd` sources (file, tree, or check).

use anyhow::{Context, Result, bail};
use beskid_analysis::MietteReportError;
use beskid_analysis::format::format_source;
use beskid_analysis::projects::ProjectKind;
use clap::Args;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

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
    let input_is_dir = fs::metadata(&args.input).with_context(|| format!("stat {}", args.input.display()))?.is_dir();

    let paths =
        collect_bd_files(&args.input, input_is_dir).with_context(|| format!("scan {}", args.input.display()))?;

    if paths.is_empty() {
        Progress(format!("beskid fmt: 0 .bd file(s) in {}", fmt_duration(started.elapsed())));
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
        write_atomically(out, &formatted)?;
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
    Progress(format!("beskid fmt: {count} of {} .bd file(s) failed in {}", paths.len(), fmt_duration(elapsed)));
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
            bail!("not formatted: {} (run `beskid fmt {}`)", path.display(), path.display());
        }
        return Ok(());
    }

    if formatted != source {
        write_atomically(path, &formatted)?;
    }
    Ok(())
}

/// Replace `path` with `contents` in one step: the text is written to a temporary file in the
/// same directory and renamed over the target, so an interrupted run never leaves a half-written
/// source. The target keeps its permissions; a symbolic link is written through, not replaced.
fn write_atomically(path: &Path, contents: &str) -> Result<()> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut file = tempfile::Builder::new()
        .prefix(".beskid-fmt-")
        .suffix(".tmp")
        .tempfile_in(dir)
        .with_context(|| format!("create a temporary file in {}", dir.display()))?;
    file.write_all(contents.as_bytes()).with_context(|| format!("write {}", file.path().display()))?;
    file.as_file().sync_all().with_context(|| format!("write {}", file.path().display()))?;
    if let Ok(metadata) = fs::metadata(&target) {
        fs::set_permissions(file.path(), metadata.permissions())
            .with_context(|| format!("set permissions of {}", file.path().display()))?;
    }
    file.persist(&target).map_err(|error| error.error).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn collect_bd_files(path: &Path, input_is_dir: bool) -> Result<Vec<PathBuf>> {
    if input_is_dir {
        let mut walk = SourceWalk::default();
        walk.directory(path)?;
        let mut paths = walk.files;
        paths.sort();
        Ok(paths)
    } else if is_bd_path(path) {
        Ok(vec![path.to_path_buf()])
    } else {
        bail!("expected a `.bd` file or directory, got {}", path.display());
    }
}

/// The `.bd` sources under a directory that belong to the user.
///
/// A directory with a project manifest (`*.bproj`) contributes only its source root (the
/// manifest's `root`), so build output (`obj/`, with the dependency and Corelib copies in
/// `obj/beskid/deps`) and `.beskid/` are never formatted. A source root equal to the manifest
/// directory (`root = "."`) is walked as the directory itself. Other directories are walked,
/// skipping build, tool and VCS directories. Symbolic links are not followed, and every directory
/// is walked at most once (by canonical path), so the walk always ends.
#[derive(Default)]
struct SourceWalk {
    visited: HashSet<PathBuf>,
    files: Vec<PathBuf>,
}

impl SourceWalk {
    /// Walk `dir` unless it was walked already. Returns false when it was.
    fn directory(&mut self, dir: &Path) -> Result<bool> {
        let canonical = fs::canonicalize(dir).with_context(|| format!("resolve directory {}", dir.display()))?;
        if !self.visited.insert(canonical.clone()) {
            return Ok(false);
        }
        let entries = directory_entries(dir)?;
        let manifests: Vec<&PathBuf> = entries
            .iter()
            .filter(|(p, kind)| kind.is_file() && p.extension().and_then(OsStr::to_str) == Some("bproj"))
            .map(|(p, _)| p)
            .collect();
        if manifests.is_empty() {
            self.entries(&entries)?;
            return Ok(true);
        }

        let mut roots: Vec<(PathBuf, PathBuf, PathBuf, bool)> = Vec::new();
        for manifest_path in manifests {
            let manifest = beskid_analysis::projects::load_manifest_from_path(manifest_path)
                .with_context(|| format!("read project manifest {}", manifest_path.display()))?;
            let root = dir.join(&manifest.project.root);
            let canonical_root = fs::canonicalize(&root).with_context(|| {
                format!("source root `{}` of {} does not exist", manifest.project.root, manifest_path.display())
            })?;
            if !canonical_root.is_dir() {
                bail!("source root `{}` of {} is not a directory", manifest.project.root, manifest_path.display());
            }
            if roots.iter().any(|(_, known, _, _)| *known == canonical_root) {
                continue;
            }
            let needs_sources = !matches!(manifest.project.kind, ProjectKind::Aggregate | ProjectKind::Bsol);
            roots.push((root, canonical_root, manifest_path.clone(), needs_sources));
        }
        for (root, canonical_root, manifest_path, needs_sources) in roots {
            let before = self.files.len();
            let walked = if canonical_root == canonical {
                self.entries(&entries)?;
                true
            } else {
                self.directory(&root)?
            };
            if walked && needs_sources && self.files.len() == before {
                bail!(
                    "project {} has no `.bd` source files under its source root {}",
                    manifest_path.display(),
                    root.display()
                );
            }
        }
        Ok(true)
    }

    fn entries(&mut self, entries: &[(PathBuf, fs::FileType)]) -> Result<()> {
        for (entry, kind) in entries {
            if kind.is_dir() {
                if !entry.file_name().is_some_and(is_skipped_dir) && !entry.join("CACHEDIR.TAG").is_file() {
                    self.directory(entry)?;
                }
            } else if kind.is_file() && is_bd_path(entry) {
                self.files.push(entry.clone());
            }
        }
        Ok(())
    }
}

/// The entries of `dir`, sorted, with their own type (a symbolic link is neither a file nor a
/// directory here, so it is never followed).
fn directory_entries(dir: &Path) -> Result<Vec<(PathBuf, fs::FileType)>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read directory {}", dir.display()))? {
        let entry = entry.with_context(|| format!("read directory {}", dir.display()))?;
        let kind = entry.file_type().with_context(|| format!("read {}", entry.path().display()))?;
        entries.push((entry.path(), kind));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

fn is_bd_path(path: &Path) -> bool {
    path.extension().and_then(OsStr::to_str).is_some_and(|ext| ext.eq_ignore_ascii_case("bd"))
}

/// Directories never walked outside a project's source root: Beskid build output and tool state,
/// version control, and third-party package and environment directories. A directory that marks
/// itself as a cache (`CACHEDIR.TAG`, as Cargo's `target/` does) is skipped too. Other names
/// (`target`, `dist`, `vendor`) can be user source directories and are walked.
fn is_skipped_dir(name: &OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(".git" | "obj" | ".beskid" | ".svn" | ".hg" | "node_modules" | ".venv" | "__pycache__")
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
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("Main.bd");
        let source = "i32 Main(){return 0;}\n";
        std::fs::write(&path, source).unwrap();
        let result =
            execute(FormatArgs { input: path.clone(), output: None, check: false, diagnostics: Default::default() });
        let actual = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        result.unwrap();
        assert_ne!(actual, source, "canonical fmt should edit the selected source without extra --write");
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
    fn v067_fmt_in_a_project_formats_only_its_source_root() {
        let root = temp_root("project");
        std::fs::create_dir_all(root.join("Src")).unwrap();
        std::fs::create_dir_all(root.join("obj/beskid/deps/Core/Src")).unwrap();
        std::fs::create_dir_all(root.join(".beskid")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(
            root.join("App.bproj"),
            "App { name = \"App\" version = \"0.1.0\" root = \"Src\" }\ntarget \"Main\" { kind = \"Lib\" entry = \"Main.bd\" }\n",
        )
        .unwrap();
        let source = "i32 Main() { return 0; }\n";
        let broken = "this is not Beskid\n";
        std::fs::write(root.join("Src/Main.bd"), source).unwrap();
        for other in ["obj/beskid/deps/Core/Src/Generated.bd", ".beskid/Cache.bd", "target/Out.bd", "Loose.bd"] {
            std::fs::write(root.join(other), broken).unwrap();
        }
        let paths = collect_bd_files(&root, true).unwrap();
        let result =
            execute(FormatArgs { diagnostics: Default::default(), input: root.clone(), output: None, check: false });
        let formatted = std::fs::read_to_string(root.join("Src/Main.bd")).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(paths, vec![root.join("Src/Main.bd")], "only the source root is formatted");
        result.expect("files outside the source root are not read");
        assert!(formatted.contains("i32 Main()\n{\n"), "{formatted}");
    }

    fn write_file(path: &std::path::Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    const BROKEN: &str = "this is not Beskid\n";

    #[test]
    fn fmt_with_root_dot_walks_the_manifest_directory_once() {
        let root = temp_root("root-dot");
        write_file(
            &root.join("App.bproj"),
            "App { name = \"App\" version = \"0.1.0\" root = \".\" }\ntarget \"Main\" { kind = \"Lib\" entry = \"Main.bd\" }\n",
        );
        write_file(&root.join("Main.bd"), "i32 Main() { return 0; }\n");
        write_file(&root.join("Lib/Util.bd"), "i32 Util() { return 1; }\n");
        for other in ["obj/beskid/deps/Core/Src/Generated.bd", ".beskid/Cache.bd"] {
            write_file(&root.join(other), BROKEN);
        }
        let paths = collect_bd_files(&root, true);
        let result =
            execute(FormatArgs { diagnostics: Default::default(), input: root.clone(), output: None, check: false });
        let formatted = std::fs::read_to_string(root.join("Lib/Util.bd")).unwrap();
        let check =
            execute(FormatArgs { diagnostics: Default::default(), input: root.clone(), output: None, check: true });
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(paths.unwrap(), vec![root.join("Lib/Util.bd"), root.join("Main.bd")]);
        result.expect("`obj/` and `.beskid/` are not formatted");
        assert!(formatted.contains("i32 Util()\n{\n"), "{formatted}");
        check.expect("`--check` passes on the formatted project");
    }

    #[test]
    fn fmt_check_with_root_dot_fails_on_an_unformatted_file() {
        let root = temp_root("root-dot-check");
        write_file(
            &root.join("App.bproj"),
            "App { name = \"App\" version = \"0.1.0\" root = \".\" }\ntarget \"Main\" { kind = \"Lib\" entry = \"Main.bd\" }\n",
        );
        write_file(&root.join("Main.bd"), "i32 Main() { return 0; }\n");
        let check =
            execute(FormatArgs { diagnostics: Default::default(), input: root.clone(), output: None, check: true });
        std::fs::remove_dir_all(&root).unwrap();
        assert!(check.is_err(), "`--check` must see the project's sources");
    }

    #[test]
    fn fmt_fails_when_a_project_root_has_no_sources() {
        let root = temp_root("empty-root");
        write_file(
            &root.join("App.bproj"),
            "App { name = \"App\" version = \"0.1.0\" root = \"Src\" }\ntarget \"Main\" { kind = \"Lib\" entry = \"Main.bd\" }\n",
        );
        std::fs::create_dir_all(root.join("Src")).unwrap();
        write_file(&root.join("Loose.bd"), "i32 Main() { return 0; }\n");
        let result =
            execute(FormatArgs { diagnostics: Default::default(), input: root.clone(), output: None, check: true });
        let missing = {
            std::fs::remove_dir_all(root.join("Src")).unwrap();
            execute(FormatArgs { diagnostics: Default::default(), input: root.clone(), output: None, check: true })
        };
        std::fs::remove_dir_all(&root).unwrap();
        let error = result.expect_err("a project without sources must fail");
        assert!(format!("{error:#}").contains("no `.bd` source files"), "{error:#}");
        let error = missing.expect_err("a missing source root must fail");
        assert!(format!("{error:#}").contains("does not exist"), "{error:#}");
    }

    #[test]
    fn fmt_walks_user_directories_named_like_tool_output() {
        let root = temp_root("user-dirs");
        for dir in ["target", "dist", "vendor"] {
            write_file(&root.join(dir).join("Main.bd"), "i32 Main() { return 0; }\n");
        }
        write_file(&root.join("cache/CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55\n");
        write_file(&root.join("cache/Out.bd"), BROKEN);
        let paths = collect_bd_files(&root, true).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        let expected: Vec<_> = ["dist", "target", "vendor"].iter().map(|d| root.join(d).join("Main.bd")).collect();
        assert_eq!(paths, expected);
    }

    #[cfg(unix)]
    #[test]
    fn fmt_does_not_follow_directory_links() {
        let root = temp_root("links");
        write_file(&root.join("Src/Main.bd"), "i32 Main() { return 0; }\n");
        std::os::unix::fs::symlink(root.join("Src"), root.join("Src/Again")).unwrap();
        std::os::unix::fs::symlink(&root, root.join("Src/Up")).unwrap();
        let paths = collect_bd_files(&root, true).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(paths, vec![root.join("Src/Main.bd")]);
    }

    #[cfg(unix)]
    #[test]
    fn fmt_writes_through_a_file_link_and_keeps_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_root("write");
        let real = root.join("Real.bd");
        write_file(&real, "i32 Main() { return 0; }\n");
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = root.join("Link.bd");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let result =
            execute(FormatArgs { diagnostics: Default::default(), input: link.clone(), output: None, check: false });
        let is_link = std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink();
        let text = std::fs::read_to_string(&real).unwrap();
        let mode = std::fs::metadata(&real).unwrap().permissions().mode() & 0o777;
        let leftovers = std::fs::read_dir(&root).unwrap().count();
        std::fs::remove_dir_all(&root).unwrap();
        result.unwrap();
        assert!(is_link, "the link stays a link");
        assert!(text.contains("i32 Main()\n{\n"), "{text}");
        assert_eq!(mode, 0o640);
        assert_eq!(leftovers, 2, "no temporary file is left behind");
    }

    #[test]
    fn v067_fmt_fails_on_a_parse_error_and_leaves_the_file_unchanged() {
        let root = temp_root("broken");
        let path = root.join("Broken.bd");
        let source = "i32 Main()\n{\n    let x: i32 = 5\n    return x;\n}\n";
        std::fs::write(&path, source).unwrap();
        let result =
            execute(FormatArgs { diagnostics: Default::default(), input: path.clone(), output: None, check: false });
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
        execute(FormatArgs { diagnostics: Default::default(), input: path.clone(), output: None, check: false })
            .unwrap();
        let formatted = std::fs::read_to_string(&path).unwrap();
        let check =
            execute(FormatArgs { diagnostics: Default::default(), input: path.clone(), output: None, check: true });
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
