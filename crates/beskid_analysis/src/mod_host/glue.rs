use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_GLUE};
use tracing::debug;

use crate::syntax::{Attribute, ContractNode, Node, Program, Spanned};

use super::types::{ContractRegistration, ModHostSession};

/// Contract id prefix shared by every `Beskid.Glue.Contracts.*` mod contract
/// (TypeMapping, SymbolEmission, LinkArgs, SignatureReader, SignatureWriter,
/// ToolchainProbe, StdioBridge).
const GLUE_CONTRACT_PREFIX: &str = "Beskid.Glue.Contracts.";

/// The three glue attributes declared in `Core/Glue/StdioBridge.bd` that mark
/// source items for glue-driven foreign interop. The 0.4 registration only
/// recognizes these names so the compiler accepts them without error; 0.5
/// drives glue generation from the collected annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlueAttributeKind {
    /// `[Glue(backend)]` — marks a type or function as a glue entry point.
    Glue,
    /// `[GlueImport(library)]` — marks a contract or function as a glue-driven foreign import.
    GlueImport,
    /// `[GlueExport(library)]` — marks a function as a glue-driven foreign export.
    GlueExport,
}

impl GlueAttributeKind {
    /// The source attribute name as declared in `StdioBridge.bd`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Glue => "Glue",
            Self::GlueImport => "GlueImport",
            Self::GlueExport => "GlueExport",
        }
    }

    /// Recognize a glue attribute by its source name, mirroring the
    /// string-based `[InternalSymbol]` recognition in `registrations`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Glue" => Some(Self::Glue),
            "GlueImport" => Some(Self::GlueImport),
            "GlueExport" => Some(Self::GlueExport),
            _ => None,
        }
    }
}

/// `true` when `name` is one of the three glue attributes. This is the 0.4
/// registration surface: the compiler accepts any attribute syntactically, and
/// this predicate is the single source of truth that distinguishes glue
/// attributes from every other attribute. 0.5 semantic handling consults it
/// before driving glue generation.
pub fn is_glue_attribute(name: &str) -> bool {
    GlueAttributeKind::from_name(name).is_some()
}

/// A source item marked with a glue attribute. The 0.4 scaffold collects these
/// for tracing; 0.5 glue generation consumes them to drive backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlueAnnotation {
    pub kind: GlueAttributeKind,
    pub item: String,
}

/// Collect every glue-attribute annotation in `program`, recursing through
/// inline modules. Mirrors `collect_internal_symbol_entry_methods` from
/// `registrations`: a syntax-only scan that recognizes attribute names by
/// string. The 0.4 scaffold does not act on the result beyond tracing.
pub fn collect_glue_annotations(program: &Spanned<Program>) -> Vec<GlueAnnotation> {
    let mut annotations = Vec::new();
    collect_glue_annotations_from_items(&program.node.items, &mut annotations);
    annotations
}

fn collect_glue_annotations_from_items(items: &[Spanned<Node>], annotations: &mut Vec<GlueAnnotation>) {
    for item in items {
        match &item.node {
            Node::TypeDefinition(definition) => {
                let type_name = definition.node.name.node.name.clone();
                push_glue_annotations(&definition.node.attributes, &type_name, annotations);
                for method in &definition.node.methods {
                    let method_name = format!("{}.{}", type_name, method.node.name.node.name);
                    push_glue_annotations(&method.node.attributes, &method_name, annotations);
                }
            }
            Node::Function(definition) => {
                push_glue_annotations(&definition.node.attributes, &definition.node.name.node.name, annotations);
            }
            Node::ContractDefinition(definition) => {
                let contract_name = definition.node.name.node.name.clone();
                push_glue_annotations(&definition.node.attributes, &contract_name, annotations);
                for contract_item in &definition.node.items {
                    if let ContractNode::MethodSignature(signature) = &contract_item.node {
                        let method_name = format!("{}.{}", contract_name, signature.node.name.node.name);
                        push_glue_annotations(&signature.node.attributes, &method_name, annotations);
                    }
                }
            }
            Node::InlineModule(module) => {
                collect_glue_annotations_from_items(&module.node.items, annotations);
            }
            _ => {}
        }
    }
}

fn push_glue_annotations(attributes: &[Spanned<Attribute>], item: &str, annotations: &mut Vec<GlueAnnotation>) {
    for attribute in attributes {
        if let Some(kind) = GlueAttributeKind::from_name(&attribute.node.name.node.name) {
            annotations.push(GlueAnnotation { kind, item: item.to_owned() });
        }
    }
}

/// Run the `mod.glue` phase over the rewritten program.
///
/// 0.4 scaffold: the phase observation runs so pipeline observers see
/// `mod.glue` start/end, and the host counts how many glue contract
/// registrations and glue-attribute annotations are present for tracing. No
/// glue contract is invoked yet — glue mod implementations land in 0.5. The
/// input program is returned unchanged, and an empty glue registration set is
/// not an error.
pub(crate) fn run_glue(
    program: Spanned<Program>,
    session: &ModHostSession,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<Spanned<Program>> {
    observe_phase_result(pipeline, MOD_GLUE, || {
        let glue_registrations: Vec<&ContractRegistration> =
            session.registrations().filter(|registration| is_glue_registration(registration)).collect();
        let glue_annotations = collect_glue_annotations(&program);

        debug!(
            glue_contract_count = glue_registrations.len(),
            glue_annotation_count = glue_annotations.len(),
            "mod.glue scaffold: phase observed, no glue contracts invoked (0.4)"
        );

        Ok(program)
    })
}

fn is_glue_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.starts_with(GLUE_CONTRACT_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_host::types::{ContractRegistration, LoadedModArtifact};
    use crate::services::parse_program_with_source_name;

    fn session_with_registrations(registrations: Vec<ContractRegistration>) -> ModHostSession {
        let loaded = vec![LoadedModArtifact {
            discovered: crate::mod_host::types::DiscoveredMod {
                dependency_name: "GlueMod".to_owned(),
                project_name: "GlueMod".to_owned(),
                project_root: std::path::PathBuf::new(),
                manifest_path: std::path::PathBuf::new(),
                source_root: std::path::PathBuf::new(),
                mod_section: None,
            },
            descriptor: None,
            registrations,
        }];
        ModHostSession::new(loaded)
    }

    fn glue_registration(contract_id: &str) -> ContractRegistration {
        ContractRegistration {
            contract_id: contract_id.to_owned(),
            type_id: "GlueMod.Impl".to_owned(),
            entry_symbol: "gluemod_entry".to_owned(),
        }
    }

    fn sample_program() -> Spanned<Program> {
        parse_program_with_source_name("Main.bd", "unit Main() { return; }\n").expect("parse")
    }

    #[test]
    fn empty_session_returns_program_unchanged() {
        let session = ModHostSession::default();
        let program = sample_program();
        let out = run_glue(program.clone(), &session, None).expect("glue scaffold");
        assert_eq!(out, program, "glue scaffold must return the input program unchanged");
    }

    #[test]
    fn counts_only_glue_contract_registrations() {
        let session = session_with_registrations(vec![
            glue_registration("Beskid.Glue.Contracts.TypeMapping"),
            glue_registration("Beskid.Glue.Contracts.StdioBridge"),
            ContractRegistration {
                contract_id: "Beskid.Compiler.Collect.Analyzer".to_owned(),
                type_id: "GlueMod.Analyzer".to_owned(),
                entry_symbol: "gluemod_analyze".to_owned(),
            },
        ]);
        let glue_count = session.registrations().filter(|r| is_glue_registration(r)).count();
        assert_eq!(glue_count, 2, "only Beskid.Glue.Contracts.* registrations are glue");
        let program = sample_program();
        let out = run_glue(program.clone(), &session, None).expect("glue scaffold");
        assert_eq!(out, program, "glue scaffold must not mutate the program even with glue registrations");
    }

    #[test]
    fn is_glue_registration_matches_all_seven_glue_contracts() {
        for contract_id in [
            "Beskid.Glue.Contracts.TypeMapping",
            "Beskid.Glue.Contracts.SymbolEmission",
            "Beskid.Glue.Contracts.LinkArgs",
            "Beskid.Glue.Contracts.SignatureReader",
            "Beskid.Glue.Contracts.SignatureWriter",
            "Beskid.Glue.Contracts.ToolchainProbe",
            "Beskid.Glue.Contracts.StdioBridge",
        ] {
            assert!(is_glue_registration(&glue_registration(contract_id)), "{contract_id} should be glue");
        }
    }

    #[test]
    fn is_glue_registration_rejects_non_glue_contracts() {
        assert!(!is_glue_registration(&glue_registration("Beskid.Compiler.Collect.Analyzer")));
    }

    #[test]
    fn is_glue_attribute_recognizes_the_three_glue_names() {
        assert!(is_glue_attribute("Glue"));
        assert!(is_glue_attribute("GlueImport"));
        assert!(is_glue_attribute("GlueExport"));
    }

    #[test]
    fn is_glue_attribute_rejects_non_glue_names() {
        assert!(!is_glue_attribute("InternalSymbol"));
        assert!(!is_glue_attribute("Extern"));
        assert!(!is_glue_attribute(""));
        assert!(!is_glue_attribute("glue"));
    }

    #[test]
    fn glue_attribute_kind_from_name_round_trips() {
        assert_eq!(GlueAttributeKind::from_name("Glue"), Some(GlueAttributeKind::Glue));
        assert_eq!(GlueAttributeKind::from_name("GlueImport"), Some(GlueAttributeKind::GlueImport));
        assert_eq!(GlueAttributeKind::from_name("GlueExport"), Some(GlueAttributeKind::GlueExport));
        assert_eq!(GlueAttributeKind::from_name("Extern"), None);
        assert_eq!(GlueAttributeKind::as_str(GlueAttributeKind::Glue), "Glue");
        assert_eq!(GlueAttributeKind::as_str(GlueAttributeKind::GlueImport), "GlueImport");
        assert_eq!(GlueAttributeKind::as_str(GlueAttributeKind::GlueExport), "GlueExport");
    }

    #[test]
    fn collect_glue_annotations_returns_empty_for_plain_program() {
        let program = sample_program();
        let annotations = collect_glue_annotations(&program);
        assert!(annotations.is_empty(), "a program without glue attributes yields no annotations");
    }

    #[test]
    fn collect_glue_annotations_collects_all_three_attributes() {
        let source = "\
[Glue(backend:\"rust\")]
unit Bridge() { return; }

[GlueExport(library:\"native\")]
unit Exported() { return; }

[GlueImport(library:\"native\")]
pub contract Foreign {
    [GlueImport(library:\"native\")]
    i32 Import();
}

pub type Wrapper {
    [Glue(backend:\"rust\")]
    unit Method() { return; }
}
";
        let program = parse_program_with_source_name("Glue.bd", source).expect("parse glue source");
        let annotations = collect_glue_annotations(&program);

        assert_eq!(annotations.len(), 5, "one annotation per glue attribute occurrence");
        assert!(annotations.contains(&GlueAnnotation { kind: GlueAttributeKind::Glue, item: "Bridge".to_owned() }));
        assert!(
            annotations.contains(&GlueAnnotation { kind: GlueAttributeKind::GlueExport, item: "Exported".to_owned() })
        );
        assert!(
            annotations.contains(&GlueAnnotation { kind: GlueAttributeKind::GlueImport, item: "Foreign".to_owned() })
        );
        assert!(
            annotations
                .contains(&GlueAnnotation { kind: GlueAttributeKind::GlueImport, item: "Foreign.Import".to_owned() })
        );
        assert!(
            annotations.contains(&GlueAnnotation { kind: GlueAttributeKind::Glue, item: "Wrapper.Method".to_owned() })
        );
    }

    #[test]
    fn run_glue_counts_glue_annotations_and_returns_program_unchanged() {
        let source = "\
[Glue(backend:\"rust\")]
unit Bridge() { return; }
";
        let program = parse_program_with_source_name("Glue.bd", source).expect("parse");
        let session = ModHostSession::default();
        let out = run_glue(program.clone(), &session, None).expect("glue scaffold");
        assert_eq!(out, program, "glue scaffold must return the input program unchanged");
        assert_eq!(collect_glue_annotations(&program).len(), 1, "annotation is recognized without error");
    }
}
