//! Output records and accumulation state for syntax-driven ISLE code generation.

use std::collections::HashMap;

use beskid_analysis::types::TypeId;
use cranelift_codegen::ir::Function;

/// One generated function at the Cranelift artifact boundary.
#[derive(Debug, Clone)]
pub struct LoweredFunction {
    pub name: String,
    pub function: Function,
}

/// External function import authorized by syntax and semantic facts.
#[derive(Debug, Clone)]
pub struct ExternImport {
    pub symbol: String,
    pub abi: Option<String>,
    pub library: Option<String>,
}

/// One exported Beskid function and its linker-visible symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportEntry {
    pub beskid_name: String,
    pub exported_symbol: String,
    pub abi: String,
}

/// Serialized type descriptor payload emitted into object or JIT data.
#[derive(Debug, Clone)]
pub struct TypeDescriptorData {
    pub size: usize,
    pub align: usize,
    pub pointer_offsets: Vec<usize>,
}

/// Complete output of the syntax → ISLE → verified CLIF path.
#[derive(Debug, Clone, Default)]
pub struct CodegenArtifact {
    pub functions: Vec<LoweredFunction>,
    pub type_descriptors: HashMap<TypeId, TypeDescriptorData>,
    pub string_literals: HashMap<String, Vec<u8>>,
    pub closure_static_plans: Vec<crate::closure_static::ClosureStaticPlan>,
    pub aggregate_static_plans: Vec<crate::aggregate_static::AggregateStaticPlan>,
    pub array_static_plans: Vec<crate::array_static::ArrayStaticPlan>,
    pub extern_imports: Vec<ExternImport>,
    /// Runtime-intrinsic imports resolved only because this artifact's compilation held the
    /// canonical runtime intrinsic capability (see `runtime_intrinsic_symbols` in
    /// `module_emission::imports`), kept separate from `extern_imports` so a JIT host can trust
    /// their provenance by construction instead of by symbol-name pattern matching. An ordinary
    /// compiled program — one whose source never held that capability — always has this empty,
    /// even if it declares an `[Extern]` FFI import that happens to share a runtime-intrinsic
    /// name; that import stays in `extern_imports` and is subject to the ordinary user-FFI
    /// authorization path, which rejects a name collision with a runtime-owned symbol.
    pub trusted_extern_imports: Vec<ExternImport>,
    pub exports: Vec<ExportEntry>,
}

/// Artifact-owned state shared by generated function emitters.
#[derive(Default)]
pub struct CodegenContext {
    pub string_literals: HashMap<String, Vec<u8>>,
    artifact_namespace: String,
    next_string_literal_id: usize,
}

impl CodegenContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_artifact_namespace(namespace: impl Into<String>) -> Self {
        Self { artifact_namespace: namespace.into(), ..Self::default() }
    }

    /// Intern bytes in the artifact's addressable literal pool.
    pub fn intern_string_literal(&mut self, bytes: &[u8]) -> String {
        let storage = if bytes.is_empty() { &[0] } else { bytes };
        for (symbol, data) in &self.string_literals {
            if data.as_slice() == storage {
                return symbol.clone();
            }
        }
        let symbol = if self.artifact_namespace.is_empty() {
            format!("__beskid_str_lit_{}", self.next_string_literal_id)
        } else {
            format!("__beskid_{}_str_lit_{}", self.artifact_namespace, self.next_string_literal_id)
        };
        self.next_string_literal_id += 1;
        self.string_literals.insert(symbol.clone(), storage.to_vec());
        symbol
    }
}

/// True when `name` fits the ASCII C-identifier / mangling alphabet every link name this crate
/// generates or emits is expected to use: `[A-Za-z_][A-Za-z0-9_]*`.
///
/// A name that fails this must never reach `Module::declare_function` / `Module::declare_data`
/// as a *final* link name -- most notably an internal trace id fragment such as `#gN:nM`, or any
/// other unmangled synthesized identifier that leaked past its own item's
/// `[Export(Symbol:"...")]` mapping (see [`object_link_symbol`]). Internal, never-linked names
/// (for example a lowering-internal `Item#syntax_...` or `Item#generic_...` bookkeeping key) are
/// expected to contain `#` and are validated only *after* [`object_link_symbol`] has resolved
/// them to their real link identity, not before.
pub fn is_valid_link_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Native object-file symbol for an emitted function.
///
/// A function with no matching `[Export(Symbol:"...")]` entry (and that is not `Main`) is not
/// meant to be publicly linkable, but it still needs *some* deterministic link name: every
/// function, exported or not, is declared into the object/JIT module (see
/// `cranelift_host::declare_user_functions_with_link_symbols_and_linkage`). That internal name
/// may be a lowering-internal bookkeeping key -- a generic specialization (`Item#generic_1_2`), a
/// synthesized fallback symbol (`Item#syntax_<unit>_<node>`), or in principle an internal trace id
/// fragment (`#gN:nM`) -- so it is only handed out as-is when it already fits the link-name
/// alphabet; otherwise it is deterministically mangled first (see
/// [`mangle_internal_link_name`]) so the raw, unmangled internal key never reaches the linker.
pub fn object_link_symbol(beskid_name: &str, exports: &[ExportEntry]) -> String {
    let logical = beskid_name.split('#').next().unwrap_or(beskid_name);
    if let Some(entry) = exports.iter().find(|entry| entry.beskid_name == logical || entry.beskid_name == beskid_name) {
        return entry.exported_symbol.clone();
    }
    if logical == "Main" {
        return "main".to_owned();
    }
    internal_link_symbol(beskid_name)
}

/// Link name for a function that is deliberately *not* resolved through `[Export(Symbol:"...")]`
/// or the `Main` -> `main` host mapping: the name itself when it already fits the link-name
/// alphabet, otherwise its deterministic [`mangle_internal_link_name`] form. Callers that bypass
/// [`object_link_symbol`] for a logical-name reason (for example an executable's unselected
/// `Main`, which must not become C `main`) use this so a raw `#`-carrying key never reaches the
/// object file.
pub fn internal_link_symbol(beskid_name: &str) -> String {
    if is_valid_link_name(beskid_name) { beskid_name.to_owned() } else { mangle_internal_link_name(beskid_name) }
}

/// Deterministically, injectively mangle a lowering-internal name into the link-name alphabet.
///
/// Every byte outside `[A-Za-z0-9]` -- including a plain `_`, so the escape introducer `_H` can
/// never occur in the output except as the start of a genuine escape token -- is rewritten to a
/// fixed-width `_H` + two uppercase hex digits + `_` token (`#` -> `_H23_`, `:` -> `_H3A_`, and so
/// on). A byte at position 0 is additionally escaped when it is an ASCII digit, since a leading
/// digit is not a valid link-name first character either. An empty name mangles to `_H00_`.
///
/// Because every emitted token is either exactly one alphanumeric character (which is never `_`)
/// or a self-contained 5-character `_H..._` escape, the output can always be re-tokenized left to
/// right by an implicit decoder that reverses this exact construction: a `_` always begins a
/// 5-character escape token, anything else is one literal byte. That left inverse existing is
/// what makes this mapping provably injective -- two distinct inputs can never mangle to the same
/// link name, so two internal items can never collide onto the same object-file symbol.
fn mangle_internal_link_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 8);
    for (index, byte) in name.bytes().enumerate() {
        let character = byte as char;
        let passthrough = character.is_ascii_alphanumeric() && !(index == 0 && character.is_ascii_digit());
        if passthrough {
            out.push(character);
        } else {
            out.push_str(&format!("_H{byte:02X}_"));
        }
    }
    if out.is_empty() {
        out.push_str("_H00_");
    }
    debug_assert!(is_valid_link_name(&out), "mangle_internal_link_name must always produce a valid link name");
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{ExportEntry, internal_link_symbol, is_valid_link_name, mangle_internal_link_name, object_link_symbol};

    #[test]
    fn rejects_names_carrying_an_internal_trace_fragment() {
        assert!(!is_valid_link_name("Widget#gen1:nod2"), "a bare `#gN:nM` trace id is not a link name");
        assert!(!is_valid_link_name("Widget#generic_1_2"), "an unresolved `#generic_` bookkeeping key is not a link name");
        assert!(!is_valid_link_name(""), "an empty name is never a valid link name");
        assert!(!is_valid_link_name("0widget"), "a leading digit is not a valid C identifier");
        assert!(!is_valid_link_name("widget name"), "whitespace is not valid in a C identifier");
    }

    #[test]
    fn accepts_ordinary_c_identifier_names() {
        assert!(is_valid_link_name("beskid_widget_construct"));
        assert!(is_valid_link_name("__beskid_type_desc_0"));
        assert!(is_valid_link_name("main"));
        assert!(is_valid_link_name("_leading_underscore"));
    }

    #[test]
    fn mangle_always_produces_a_valid_link_name() {
        for name in [
            "Widget#gen1:nod2",
            "Widget#generic_1_2",
            "Widget#syntax_App_7",
            "",
            "0widget",
            "already_valid_name",
            "_H23_",
            "_Hfoo",
        ] {
            let mangled = mangle_internal_link_name(name);
            assert!(is_valid_link_name(&mangled), "mangled `{name}` -> `{mangled}` must be a valid link name");
        }
    }

    #[test]
    fn mangle_is_injective_across_colliding_looking_inputs() {
        // These are deliberately chosen to probe the `_H` escape-introducer collision the mangling
        // scheme must defend against: distinct sources that could plausibly mangle to the same
        // output if underscores were left unescaped or the escape prefix were not itself escaped.
        let candidates = [
            "Widget#generic_1_2",
            "Widget#generic_1:2",
            "Widget_Hgeneric_1_2",
            "Widget_H23_generic_1_2",
            "_H23_",
            "#",
            "_H23",
            "H23_",
        ];
        let mangled = candidates.iter().map(|name| mangle_internal_link_name(name)).collect::<Vec<_>>();
        let unique: HashSet<_> = mangled.iter().cloned().collect();
        assert_eq!(unique.len(), mangled.len(), "distinct inputs must never mangle to the same link name: {mangled:?}");
    }

    #[test]
    fn object_link_symbol_mangles_an_unexported_generic_specialization_instead_of_rejecting_it() {
        let mangled = object_link_symbol("Widget#generic_1_2", &[]);
        assert!(is_valid_link_name(&mangled), "an unexported generic specialization must still get a valid link name");
        assert_ne!(mangled, "Widget#generic_1_2", "the raw internal key must not be handed out unchanged");
    }

    #[test]
    fn object_link_symbol_leaves_an_explicit_export_untouched() {
        let exports = vec![ExportEntry {
            beskid_name: "Widget".to_owned(),
            exported_symbol: "beskid_widget_entry".to_owned(),
            abi: "C".to_owned(),
        }];
        assert_eq!(object_link_symbol("Widget#generic_1_2", &exports), "beskid_widget_entry");
    }

    #[test]
    fn internal_link_symbol_mangles_main_without_mapping_it_to_c_main() {
        assert_eq!(internal_link_symbol("Main#0"), "Main_H23_0");
        assert_eq!(internal_link_symbol("beskid_widget_helper"), "beskid_widget_helper");
    }

    #[test]
    fn object_link_symbol_leaves_an_already_clean_internal_name_unchanged() {
        assert_eq!(object_link_symbol("beskid_widget_helper", &[]), "beskid_widget_helper");
    }
}
