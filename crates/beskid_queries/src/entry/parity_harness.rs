//! Shared fixture harness of the query-gate parity tests (`parity_tests.rs` and the per-class
//! modules beside it): judge one or more own units of a synthetic entry-less library through
//! `semantic_diagnostics_for_roots` alone. No legacy checker runs here.

use std::sync::Arc;

use beskid_analysis::projects::{
    AssemblyDiscovery, AssemblyRootSet, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::{SemanticFactFinding, parse_program_with_source_name, synthetic_compile_plan_for_source};
use beskid_analysis::syntax::SyntaxGenerationId;
use beskid_analysis::syntax_query::SyntaxIndex;

use super::semantic_diagnostics_for_roots;
use crate::BeskidDatabase;

/// Judge `source` as the single own root of a library through the query gate only.
pub(super) fn root_findings(source: &str, generation: u64) -> Vec<SemanticFactFinding> {
    root_findings_with_glue(source, generation, &[])
}

/// [`root_findings`] for an assembly whose host manifest declares `glue_libraries` as Rust Glue
/// owner blocks.
pub(super) fn root_findings_with_glue(
    source: &str,
    generation: u64,
    glue_libraries: &[&str],
) -> Vec<SemanticFactFinding> {
    unit_findings_with_glue(&[("Main.bd", source)], generation, glue_libraries)
}

/// Judge every unit of `units` (`(path relative to the source root, source)`) as the own roots of
/// one entry-less library. The first unit is the assembly entry; findings carry the index of the
/// unit they are located in, in `units` order.
pub(super) fn unit_findings(units: &[(&str, &str)], generation: u64) -> Vec<SemanticFactFinding> {
    unit_findings_with_glue(units, generation, &[])
}

pub(super) fn unit_findings_with_glue(
    units: &[(&str, &str)],
    generation: u64,
    glue_libraries: &[&str],
) -> Vec<SemanticFactFinding> {
    assert!(!units.is_empty(), "a parity fixture needs at least one unit");
    let root = std::env::temp_dir().join(format!("beskid_query_check_parity_{}_{generation}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let source_root = root.join("src");
    std::fs::create_dir_all(&source_root).expect("library source root");
    let generation = SyntaxGenerationId(generation);
    let source_units = units
        .iter()
        .map(|(relative, source)| {
            let path = source_root.join(relative);
            std::fs::create_dir_all(path.parent().expect("unit parent")).expect("unit directory");
            std::fs::write(&path, source).expect("library source");
            SourceUnit {
                logical_name: (*relative).to_string(),
                origin_path: path.clone(),
                program: parse_program_with_source_name(path.to_str().expect("utf-8 path"), source)
                    .unwrap_or_else(|error| panic!("fixture must parse: {error}\n{source}")),
                path,
                source: (*source).to_string(),
            }
        })
        .collect::<Vec<_>>();
    let plan = synthetic_compile_plan_for_source(&source_units[0].path);
    let roots = EffectiveCompilationRoots {
        host: RootEntry { dependency_name: None, source_root: source_root.clone() },
        dependencies: Vec::new(),
    };
    let indexes = source_units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, generation)).collect::<Vec<_>>();
    let index = Arc::new(ModuleIndex::build(&source_units, &indexes, &roots, &plan));
    let program = source_units[0].program.clone();
    let own_paths = source_units.iter().map(|unit| unit.path.clone()).collect::<Vec<_>>();
    let assembly = ProgramAssembly::new(
        roots,
        Arc::new(source_units),
        0,
        AssemblyDiscovery::ImportClosure,
        index,
        false,
        generation,
    )
    .with_root_set(AssemblyRootSet::OwnUnits(Arc::from(own_paths)))
    .with_glue_libraries(glue_libraries.iter().map(|library| (*library).to_string()).collect::<Vec<_>>().into());
    let mut db = BeskidDatabase::default();
    let findings = semantic_diagnostics_for_roots(&mut db, None, &assembly, &program).unwrap_or_else(|error| {
        panic!("query gate must judge the fixture: {error}\n{}", units.iter().map(|(_, s)| *s).collect::<Vec<_>>().join("\n---\n"))
    });
    let _ = std::fs::remove_dir_all(&root);
    findings
}

/// The diagnostic codes of `findings`, in finding order.
pub(super) fn codes(findings: &[SemanticFactFinding]) -> Vec<&'static str> {
    findings.iter().map(|finding| finding.kind.code()).collect()
}

/// Whether `findings` carries a finding of `code` whose `SemanticIssueKind` variant name starts
/// with `variant` (the `Debug` rendering of the kind), optionally located in unit `unit`.
pub(super) fn reports_kind(
    findings: &[SemanticFactFinding],
    variant: &str,
    code: &str,
    unit: Option<usize>,
) -> bool {
    findings.iter().any(|finding| {
        finding.kind.code() == code
            && format!("{:?}", finding.kind).starts_with(variant)
            && unit.is_none_or(|unit| finding.unit == unit)
    })
}

/// Assert, over a table of `(class, variant, code, source)` single-unit fixtures, that the query
/// gate alone reports every class with its exact variant and code. Every class is evaluated
/// before the assertion so one run names every failing class. Generations are
/// `base + offset`.
pub(super) fn assert_covered_kinds(table: &[(&str, &str, &str, &str)], base: u64) {
    let mut failures = Vec::new();
    for (offset, (class, variant, code, source)) in table.iter().enumerate() {
        let findings = root_findings(source, base + offset as u64);
        if !reports_kind(&findings, variant, code, None) {
            failures.push(format!("{class}: expected {variant} ({code}), got {:?}\n  {source}", codes(&findings)));
        }
    }
    assert!(failures.is_empty(), "query gate misses {} class(es):\n{}", failures.len(), failures.join("\n"));
}

/// Assert that every `(class, source)` single-unit fixture produces no finding at all.
pub(super) fn assert_silent(table: &[(&str, &str)], base: u64) {
    let mut failures = Vec::new();
    for (offset, (class, source)) in table.iter().enumerate() {
        let findings = root_findings(source, base + offset as u64);
        if !findings.is_empty() {
            failures.push(format!("{class}: expected no finding, got {findings:?}\n  {source}"));
        }
    }
    assert!(failures.is_empty(), "query gate reports {} false positive(s):\n{}", failures.len(), failures.join("\n"));
}
