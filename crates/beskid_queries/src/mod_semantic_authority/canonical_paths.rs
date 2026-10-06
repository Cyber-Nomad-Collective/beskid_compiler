//! Read-only routes from an issued caller reference to current embedded Corelib.
use super::*;
use beskid_analysis::syntax::{ContractDefinition, EnumDefinition, FunctionDefinition, TypeDefinition};
use std::collections::HashSet;

// A closed source declaration catalog, not a caller-selected filesystem lookup.
fn admitted_selector(selector: &str) -> bool {
    matches!(
        selector,
        "Core.Results.Result"
            | "Core.Optional.Option"
            | "Core.Collections.Array.Append"
            | "Core.Collections.Array.Len"
            | "Core.Collections.List.New"
            | "Core.Collections.List.Append"
            | "Core.Collections.Map.New"
            | "Core.Collections.Map.Set"
            | "Core.Collections.Map.Contains"
            | "Core.String.Core.SortOrdinal"
            | "Core.Serialization.Contracts.Serializable"
            | "Core.Serialization.Contracts.Encoder"
            | "Core.Serialization.Contracts.Decoder"
            | "Core.Serialization.Reader.Reader"
            | "Core.Serialization.Reader.FieldDecoder"
            | "Core.Serialization.Limits.SerializationLimits"
            | "Core.Serialization.Errors.SerializationError"
            | "Core.Serialization.Errors.Diagnostic"
            | "Core.Serialization.Errors.NumericError"
            | "Core.Serialization.Descriptors.CompiledShape"
            | "Core.Numeric.FloatBits.ToBits32"
            | "Core.Numeric.FloatBits.FromBits32"
            | "Core.Numeric.FloatBits.ToBits64"
            | "Core.Numeric.FloatBits.FromBits64"
    )
}

impl ModSemanticQueryAuthority<'_> {
    pub(super) fn canonical_paths(
        &self,
        invocation: u64,
        reference: &ModSyntaxNodeRef,
        requested: &[String],
    ) -> Result<Vec<ModCanonicalPath>, ModSemanticError> {
        self.validate_registration()?;
        let caller = self.syntax_key(invocation, reference)?;
        if requested.len() > 128 {
            return Err(Self::error("canonical route request exceeds 128 selectors"));
        }
        let mut selected = HashSet::new();
        let mut output = Vec::with_capacity(requested.len());
        for selector in requested {
            if !admitted_selector(selector) || !selected.insert(selector) {
                return Err(Self::error("unknown or repeated canonical route selector"));
            }
            let (module, name) = selector.rsplit_once('.').ok_or_else(|| Self::error("invalid canonical selector"))?;
            let logical_source = match module {
                "Core.Results" => "Core/Results/Results.bd".to_owned(),
                "Core.Optional" => "Core/Optional/Option.bd".to_owned(),
                _ => format!("{}.bd", module.replace('.', "/")),
            };
            let mut declarations = Vec::new();
            for unit in self.assembly.units.iter() {
                let unit_id = SourceUnitId::new(self.db, unit.path.clone());
                let syntax = self.db.syntax_unit(unit_id).ok_or_else(|| Self::error("unregistered route unit"))?;
                for metadata in syntax.syntax_index(self.db).metadata() {
                    let key = AstNodeKey { unit: unit_id, generation: self.assembly.generation, node: metadata.id };
                    if crate::canonical_corelib_source_path(self.db, key).as_deref() != Some(logical_source.as_str()) {
                        continue;
                    }
                    let Some(node) =
                        syntax.syntax_index(self.db).node_at(syntax.expanded_program(self.db), metadata.id)
                    else {
                        continue;
                    };
                    let matches = node.of::<TypeDefinition>().is_some_and(|value| value.name.node.name == name)
                        || node.of::<EnumDefinition>().is_some_and(|value| value.name.node.name == name)
                        || node.of::<ContractDefinition>().is_some_and(|value| value.name.node.name == name)
                        || node.of::<FunctionDefinition>().is_some_and(|value| value.name.node.name == name);
                    if matches {
                        declarations.push(key);
                    }
                }
            }
            let [declaration] = declarations.as_slice() else {
                return Err(Self::error("canonical route declaration is absent or ambiguous"));
            };
            let registry = self.db.syntax_dependency_registry().lock().expect("syntax dependency registry");
            let mut routes = Vec::new();
            for ((generation, route), _) in registry.modules.iter() {
                if *generation != caller.generation {
                    continue;
                }
                let Some(units) = registry.visible_module_units(caller.unit, caller.generation, route) else {
                    continue;
                };
                // The complete route must identify one physical source unit. A namespace
                // containing several candidates cannot provide declaration correspondence.
                if units != [declaration.unit] {
                    continue;
                }
                let mut full = route.clone();
                full.push(name.to_owned());
                routes.push(full);
            }
            // Multiple aliases to the same exact declaration are equivalent routes.
            // Prefer the shortest visible route, then lexical order; no prefix is guessed.
            routes.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
            routes.dedup();
            let segments = routes
                .into_iter()
                .next()
                .ok_or_else(|| Self::error("canonical declaration has no caller-visible route"))?;
            output.push(ModCanonicalPath { logical_name: selector.clone(), segments });
        }
        Ok(output)
    }
}
