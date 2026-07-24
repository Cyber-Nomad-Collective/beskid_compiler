use std::collections::HashSet;
use std::path::Path;

use beskid_analysis::services::analyze_program;

fn analyze_composition(source: &str) -> Vec<beskid_analysis::SemanticDiagnostic> {
    analyze_program(Path::new("composition.bd"), source).expect("analyze composition source")
}

fn composition_codes(source: &str) -> HashSet<String> {
    analyze_composition(source).into_iter().filter_map(|diagnostic| diagnostic.code).collect()
}

fn codes_in_e17xx_range(codes: &HashSet<String>) -> HashSet<String> {
    codes.iter().filter(|code| code.starts_with("E17")).cloned().collect()
}

#[test]
fn composition_reports_missing_launch_host() {
    let source = r#"
host AppHost() : ConsoleHost {
    registry {
        single Storage for StorageContract;
    }
}

i32 Main() {
    return 0;
}
"#;
    let codes = composition_codes(source);
    assert_eq!(codes_in_e17xx_range(&codes), HashSet::from(["E1701".to_string()]));
    let diags = analyze_composition(source);
    let e1701 = diags.iter().find(|d| d.code.as_deref() == Some("E1701")).expect("E1701 diagnostic");
    assert!(!e1701.span.is_empty(), "missing launch host should anchor to program span");
}

#[test]
fn composition_reports_unknown_launch_host() {
    let source = r#"
host AppHost() : ConsoleHost {
    registry {
        single Logger;
    }
}

i32 Main() {
    launch MissingHost();
    return 0;
}
"#;
    let codes = composition_codes(source);
    assert_eq!(codes_in_e17xx_range(&codes), HashSet::from(["E1709".to_string()]));
    let diags = analyze_composition(source);
    let e1709 = diags.iter().find(|d| d.code.as_deref() == Some("E1709")).expect("E1709 diagnostic");
    let launch_offset = source.find("launch").expect("launch keyword");
    let span_start = e1709.span.offset();
    let span_end = span_start.saturating_add(e1709.span.len().max(1));
    assert!(
        span_start <= launch_offset && launch_offset < span_end,
        "E1709 should cover launch site, span={span_start}..{span_end} launch_at={launch_offset}"
    );
}

#[test]
fn composition_reports_multiple_launch_hosts() {
    let source = r#"
host AppHost() : ConsoleHost {
    registry {
        single Logger;
    }
}

i32 Main() {
    launch AppHost();
    launch AppHost();
    return 0;
}
"#;
    let codes = composition_codes(source);
    assert!(codes.contains("E1702"), "expected E1702 for duplicate launch, got {codes:?}");
    let second_launch = source.rfind("launch").expect("second launch");
    let diags = analyze_composition(source);
    let e1702 = diags.iter().find(|d| d.code.as_deref() == Some("E1702")).expect("E1702 diagnostic");
    let span_start = e1702.span.offset();
    let span_end = span_start.saturating_add(e1702.span.len().max(1));
    assert!(span_start <= second_launch && second_launch < span_end, "E1702 should anchor to the duplicate launch");
}

#[test]
fn composition_reports_unresolved_inject() {
    let source = r#"
host AppHost() {
    registry {
        single Worker;
    }
}

type Logger {
    i32 value
}

type Worker {
    inject Logger logger
}

i32 Main() {
    launch AppHost();
    return 0;
}
"#;
    let codes = composition_codes(source);
    assert_eq!(codes_in_e17xx_range(&codes), HashSet::from(["E1704".to_string()]));
}

#[test]
fn composition_reports_dependency_cycle() {
    let source = r#"
host AppHost() {
    registry {
        single Alpha;
        single Beta;
    }
}

type Alpha {
    inject Beta beta
}

type Beta {
    inject Alpha alpha
}

i32 Main() {
    launch AppHost();
    return 0;
}
"#;
    let codes = composition_codes(source);
    assert!(codes.contains("E1703"), "expected dependency cycle E1703, got {codes:?}");
}
