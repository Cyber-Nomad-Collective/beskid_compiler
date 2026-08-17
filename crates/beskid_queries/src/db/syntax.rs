use std::sync::Arc;
use std::sync::atomic::Ordering;

use salsa::Setter;

use crate::inputs::ProjectSession;
use crate::semantic_contract::{SemanticError, SourceUnitId, SyntaxUnitInput, SyntaxUnitRevision};

use super::{BeskidDatabase, Db};

impl BeskidDatabase {
    /// Return the single registered Salsa revision input for `unit`.
    pub fn syntax_unit(&self, unit: SourceUnitId) -> Option<SyntaxUnitInput> {
        Db::syntax_unit(self, unit)
    }

    /// Borrow the plain-Rust syntax-unit registry (used by persistence rehydration).
    #[allow(dead_code)]
    pub(crate) fn syntax_unit_registry(&self) -> &std::sync::Mutex<super::SyntaxUnitRegistry> {
        &self.syntax_unit_registry
    }

    /// Return the registered input for `unit`, creating it when first observed.
    pub fn ensure_syntax_unit(
        &mut self,
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
    ) -> Result<SyntaxUnitInput, SemanticError> {
        let source = self.file_text(unit.path(self)).map(|file| file.text(self).clone()).ok_or_else(|| {
            SemanticError::new(format!("source text is not registered for {}", unit.path(self).display()))
        })?;
        let source_fingerprint = Arc::<str>::from(beskid_artifacts::content_fingerprint(&source));
        if let Some(input) = self.syntax_unit(unit) {
            self.validate_existing_registration(input, project, generation, &source_fingerprint)?;
            if input.source_fingerprint(self) == &source_fingerprint {
                return Ok(input);
            }
        }
        let program = self.parse_and_expand(unit, &source)?;
        self.register_expanded_syntax(project, unit, generation, source_fingerprint, Arc::new(program))
    }

    /// Register an already expanded source unit without reparsing away mod rewrites.
    pub fn ensure_expanded_syntax_unit(
        &mut self,
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source: String,
        expanded_program: Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>>,
    ) -> Result<SyntaxUnitInput, SemanticError> {
        let source_fingerprint = Arc::<str>::from(beskid_artifacts::content_fingerprint(&source));
        let tree_fingerprint = Arc::<str>::from(expanded_syntax_fingerprint(&expanded_program)?);
        if let Some(input) = self.syntax_unit(unit) {
            self.validate_existing_registration(input, project, generation, &source_fingerprint)?;
            if input.source_fingerprint(self) == &source_fingerprint
                && input.revision(self).tree_fingerprint == tree_fingerprint
            {
                return Ok(input);
            }
        }
        let input = self.register_expanded_syntax(project, unit, generation, source_fingerprint, expanded_program)?;
        self.ensure_file_text(unit.path(self).clone(), source);
        Ok(input)
    }

    /// Parse, expand, register, and invalidate one edited source as a single semantic update.
    pub fn update_syntax_source(
        &mut self,
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source: String,
    ) -> Result<SyntaxUnitInput, SemanticError> {
        let source_fingerprint = Arc::<str>::from(beskid_artifacts::content_fingerprint(&source));
        if let Some(input) = self.syntax_unit(unit) {
            self.validate_existing_registration(input, project, generation, &source_fingerprint)?;
            if input.source_fingerprint(self) == &source_fingerprint {
                return Ok(input);
            }
        }
        let program = self.parse_and_expand(unit, &source)?;
        let input = self.register_expanded_syntax(project, unit, generation, source_fingerprint, Arc::new(program))?;
        self.ensure_file_text(unit.path(self).clone(), source);
        Ok(input)
    }

    fn register_expanded_syntax(
        &mut self,
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source_fingerprint: Arc<str>,
        expanded_program: Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>>,
    ) -> Result<SyntaxUnitInput, SemanticError> {
        let tree_fingerprint = Arc::<str>::from(expanded_syntax_fingerprint(&expanded_program)?);
        let registry = Arc::clone(&self.syntax_unit_registry);
        let mut registry = registry.lock().expect("syntax unit registry");
        if let Some(input) = registry.get(&unit).copied() {
            self.validate_existing_registration(input, project, generation, &source_fingerprint)?;
            let current = input.revision(self);
            if current.tree_fingerprint_history.iter().any(|fingerprint| fingerprint == &tree_fingerprint) {
                return Err(SemanticError::new(
                    "expanded syntax cannot reuse a tree fingerprint from an existing generation",
                ));
            }
            let syntax_index =
                Arc::new(beskid_analysis::syntax_query::SyntaxIndex::from_program(&expanded_program, generation));
            self.syntax_index_build_count.fetch_add(1, Ordering::Relaxed);
            let mut source_fingerprint_history = current.source_fingerprint_history.to_vec();
            source_fingerprint_history.push(Arc::clone(&source_fingerprint));
            let mut tree_fingerprint_history = current.tree_fingerprint_history.to_vec();
            tree_fingerprint_history.push(Arc::clone(&tree_fingerprint));
            input.set_revision(self).to(Arc::new(SyntaxUnitRevision {
                generation,
                expanded_program,
                syntax_index,
                source_fingerprint,
                tree_fingerprint,
                source_fingerprint_history: source_fingerprint_history.into(),
                tree_fingerprint_history: tree_fingerprint_history.into(),
            }));
            return Ok(input);
        }
        let syntax_index =
            Arc::new(beskid_analysis::syntax_query::SyntaxIndex::from_program(&expanded_program, generation));
        self.syntax_index_build_count.fetch_add(1, Ordering::Relaxed);
        let input = SyntaxUnitInput::new(
            self,
            project,
            unit,
            Arc::new(SyntaxUnitRevision {
                generation,
                expanded_program,
                syntax_index,
                source_fingerprint_history: Arc::from([Arc::clone(&source_fingerprint)]),
                tree_fingerprint_history: Arc::from([Arc::clone(&tree_fingerprint)]),
                source_fingerprint,
                tree_fingerprint,
            }),
        );
        registry.insert(unit, input);
        Ok(input)
    }

    fn validate_existing_registration(
        &self,
        input: SyntaxUnitInput,
        project: ProjectSession,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source_fingerprint: &Arc<str>,
    ) -> Result<(), SemanticError> {
        if input.project(self) != project {
            return Err(SemanticError::new("a source unit cannot be reassigned to another project session"));
        }
        let current_generation = input.generation(self);
        let source_changed = input.source_fingerprint(self) != source_fingerprint;
        if generation.0 < current_generation.0 {
            return Err(SemanticError::new("syntax generation cannot regress"));
        }
        if source_changed && generation.0 == current_generation.0 {
            return Err(SemanticError::new("changed syntax requires a strictly newer generation"));
        }
        if source_changed
            && input
                .revision(self)
                .source_fingerprint_history
                .iter()
                .any(|fingerprint| fingerprint == source_fingerprint)
        {
            return Err(SemanticError::new("source syntax cannot resurrect a fingerprint from an earlier generation"));
        }
        if !source_changed && generation != current_generation {
            return Err(SemanticError::new("unchanged syntax cannot be relabeled with a different generation"));
        }
        Ok(())
    }

    fn parse_and_expand(
        &self,
        unit: SourceUnitId,
        source: &str,
    ) -> Result<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>, SemanticError> {
        self.syntax_parse_count.fetch_add(1, Ordering::Relaxed);
        let source_name = unit.path(self).display().to_string();
        let program = beskid_analysis::services::parse_program_with_source_name(&source_name, source)
            .map_err(|error| SemanticError::new(format!("failed to parse {source_name}: {error}")))?;
        let expanded = beskid_analysis::macros::expand_program_with_diagnostics(
            program,
            beskid_analysis::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
            &source_name,
            source,
        );
        let errors = expanded
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == beskid_analysis::analysis::Severity::Error)
            .map(|diagnostic| format!("failed to expand {source_name}: {}", diagnostic.message))
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return Err(SemanticError::from_diagnostics(errors));
        }
        Ok(expanded.program)
    }

    #[doc(hidden)]
    pub fn syntax_authority_counts(&self) -> (u64, u64) {
        (self.syntax_parse_count.load(Ordering::Relaxed), self.syntax_index_build_count.load(Ordering::Relaxed))
    }
}

fn expanded_syntax_fingerprint(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> Result<String, SemanticError> {
    let mut structural = serde_json::to_value(program)
        .map_err(|error| SemanticError::new(format!("failed to fingerprint expanded syntax: {error}")))?;
    remove_span_fields(&mut structural);
    let encoded = serde_json::to_string(&structural)
        .map_err(|error| SemanticError::new(format!("failed to fingerprint expanded syntax: {error}")))?;
    Ok(beskid_artifacts::content_fingerprint(&encoded))
}

fn remove_span_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                remove_span_fields(value);
            }
        }
        serde_json::Value::Object(fields) => {
            fields.remove("span");
            for value in fields.values_mut() {
                remove_span_fields(value);
            }
        }
        _ => {}
    }
}
