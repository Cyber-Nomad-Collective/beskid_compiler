//! Inventory of every `SemanticError::unavailable` query name (design
//! `docs/superpowers/specs/2026-09-23-production-semantic-diagnostics-design.md`, section 3 and
//! section 4 slice 8).
//!
//! `unavailable(name)` means a compiler gap: a fact that cannot be decided because its port is
//! incomplete. After the reachability-scoped legality gate has passed, any such gap that reaches
//! module emission is reported as internal error E2101 (or E2102 for a missing ISLE rule), never
//! as a user diagnostic. Some query families also fail for ordinary user errors; for those, a
//! legality fact reports the user error first, so it never reaches the internal-error path.
//!
//! Every name therefore appears in exactly one list below: `LEGALITY_MAPPED` names the legality
//! fact that turns the family's user-error shapes into coded findings, and `KNOWN_GAPS` names a
//! genuine port gap and its tracker reference. A new `unavailable("...")` literal without an
//! entry fails this test, and so does an entry whose literal no longer exists. A dotted name
//! (`aggregate_field_access.result`) is one failure point of the family before the first dot.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Query families whose user-error shapes a legality fact reports before lowering.
const LEGALITY_MAPPED: &[(&str, &str)] = &[
    ("abi_type", "unresolved_type_reference (E1201)"),
    ("aggregate_field_access", "member_reference_legality (E1211)"),
    ("aggregate_literal", "member_reference_legality (E1211, E1212)"),
    ("aggregate_layout", "unresolved_type_reference (E1201)"),
    ("call_abi_signature", "call_arity_mismatch (E1204), generic_parameter_conflict (E1229)"),
    ("call_lowering", "unresolved_call_target (E1101, E1108)"),
    ("dead_collection_growth", "dead_collection_growth (E1231)"),
    ("enum_constructor", "member_reference_legality (E1301, E1302)"),
    ("enum_layout", "unresolved_type_reference (E1201)"),
    ("enum_match", "member_reference_legality (E1301), match_exhaustiveness (E1304)"),
    ("generic_call_instantiation", "unresolved_call_target (E1203)"),
    ("mutable_local_assignment", "immutable_local_assignment (E1214)"),
    ("pattern_binding", "member_reference_legality (E1307)"),
    ("scoped_cleanup_layout", "scoped_cleanup (E1230)"),
    ("try_expression", "try_expression_fact through the prepare-spine fact authority (E1222)"),
];

/// Tracker reference for the known-gap families: the follow-up task of the OpenSpec change that
/// owns this inventory. Each family is a port gap, not a user error.
const GAP_TRACKER: &str = "openspec add-reachability-scoped-semantic-legality-gate task 5.3";

/// Query families that are genuine port gaps (internal error E2101 when they reach emission).
const KNOWN_GAPS: &[&str] = &[
    "array_index_element_abi_type",
    "binary_operand_abi_type",
    "block_statement_nodes",
    "call_argument_abi_type",
    "call_arguments",
    "callable_signature",
    "capture_storage",
    "cast_intents",
    "closure_environment",
    "closure_signature",
    "collection_operation",
    "contextual_integer_literal_abi_type",
    "contract_argument",
    "contract_conformance",
    "contract_method",
    "contract_parameter",
    "contract_witness",
    "corelib_value_service_result",
    "empty_array_literal_element_abi_type",
    "empty_array_literal_element_specialization",
    "enum_constructor_specialization",
    "enum_constructor_template",
    "enum_match_specialization",
    "for_iterator_element_type",
    "generic_nominal_method_receiver",
    "generic_receiver_instantiation",
    "generic_source_type_identity",
    "generic_specialization_identity",
    "generic_specialization_instance",
    "item_signature",
    "local_slot",
    "managed_reference_kind",
    "node_type",
    "nominal_field_projection",
    "pattern_binding_specialization",
    "primitive_numeric_conversion",
    "range_for_fact",
    "reachable_items",
    "runtime_intrinsic",
    "source_expression_type",
    "spawn_handle_type",
    "spawn_legality",
    "spawn_target",
    "test_statement_nodes",
    "value_abi_type",
];

/// Files allowed to call `SemanticError::unavailable` with a non-literal name, because they
/// forward a literal that is itself inventoried (`PathCallResolution::Unavailable("...")`).
const FORWARDING_SITES: &[&str] = &["src/semantic_contract/calls/resolution.rs"];

const LITERAL_PREFIXES: &[&str] = &[
    "unavailable(\"",
    "unavailable_at(\"",
    "PathCallResolution::Unavailable(\"",
    "PathCallResolution::UnresolvedTarget(\"",
];

fn source_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read source directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// Every inventoried query name, with the files that construct it.
fn inventory() -> (BTreeMap<String, BTreeSet<String>>, Vec<String>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    source_files(&root.join("src"), &mut files);
    let mut names = BTreeMap::<String, BTreeSet<String>>::new();
    let mut non_literal = Vec::new();
    for file in files {
        let relative = file.strip_prefix(root).expect("relative path").to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&file).expect("read source");
        for prefix in LITERAL_PREFIXES {
            for (index, _) in text.match_indices(prefix) {
                let rest = &text[index + prefix.len()..];
                let name = &rest[..rest.find('"').expect("closing quote")];
                // `family.detail` names one failure point of a query family; the family is inventoried.
                let family = name.split('.').next().unwrap_or(name);
                names.entry(family.to_owned()).or_default().insert(relative.clone());
            }
        }
        for (index, _) in text.match_indices("SemanticError::unavailable(") {
            let rest = &text[index + "SemanticError::unavailable(".len()..];
            if !rest.starts_with('"') && !FORWARDING_SITES.contains(&relative.as_str()) {
                non_literal.push(format!("{relative}: {}", rest.lines().next().unwrap_or_default()));
            }
        }
    }
    (names, non_literal)
}

#[test]
fn every_unavailable_query_name_is_mapped_to_a_legality_fact_or_a_known_gap() {
    let (names, non_literal) = inventory();
    assert!(!names.is_empty(), "the inventory scan found no `unavailable` construction site");
    assert!(non_literal.is_empty(), "non-literal `unavailable` names cannot be inventoried: {non_literal:#?}");
    let mapped = LEGALITY_MAPPED.iter().map(|(name, _)| *name).collect::<BTreeSet<_>>();
    let gaps = KNOWN_GAPS.iter().copied().collect::<BTreeSet<_>>();
    let both = mapped.intersection(&gaps).collect::<Vec<_>>();
    assert!(both.is_empty(), "a query family is either legality-mapped or a known gap, not both: {both:?}");
    let unlisted = names
        .iter()
        .filter(|(name, _)| !mapped.contains(name.as_str()) && !gaps.contains(name.as_str()))
        .map(|(name, files)| format!("{name} (constructed in {files:?})"))
        .collect::<Vec<_>>();
    assert!(
        unlisted.is_empty(),
        "new `unavailable` query names need a legality fact (LEGALITY_MAPPED) or a known-gap entry with \
         a tracker reference ({GAP_TRACKER}): {unlisted:#?}"
    );
}

#[test]
fn every_inventory_entry_still_names_a_constructed_query() {
    let (names, _) = inventory();
    let stale = LEGALITY_MAPPED
        .iter()
        .map(|(name, _)| *name)
        .chain(KNOWN_GAPS.iter().copied())
        .filter(|name| !names.contains_key(*name))
        .collect::<Vec<_>>();
    assert!(stale.is_empty(), "inventory entries without a construction site must be removed: {stale:?}");
}
