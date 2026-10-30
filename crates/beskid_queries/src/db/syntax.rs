use std::sync::Arc;
use std::sync::atomic::Ordering;

use salsa::Setter;

use crate::inputs::ProjectSession;
use crate::semantic_contract::{SemanticError, SourceUnitId, SyntaxUnitInput, SyntaxUnitRevision};

use super::syntax_fingerprint::expanded_syntax_fingerprint;
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
            self.validate_existing_registration(input, project, generation, &source_fingerprint, false)?;
            if input.source_fingerprint(self) == &source_fingerprint {
                return Ok(input);
            }
        }
        let program = self.parse_and_expand(unit, &source)?;
        let tree_fingerprint = Arc::<str>::from(expanded_syntax_fingerprint(&program));
        self.register_expanded_syntax(
            project,
            unit,
            generation,
            source_fingerprint,
            tree_fingerprint,
            Arc::new(program),
            false,
        )
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
        match self.current_expanded_syntax_unit(project, unit, generation, &source, &expanded_program)? {
            ExpandedRegistration::Current(input) => Ok(input),
            ExpandedRegistration::Changed { source_fingerprint, tree_fingerprint } => {
                let input = self.register_expanded_syntax(
                    project,
                    unit,
                    generation,
                    source_fingerprint,
                    tree_fingerprint,
                    expanded_program,
                    true,
                )?;
                self.ensure_file_text(unit.path(self).clone(), source);
                Ok(input)
            }
        }
    }

    /// [`Self::ensure_expanded_syntax_unit`] for a borrowed assembly unit: the source and tree are
    /// copied only when the registration actually changes.
    pub(crate) fn ensure_borrowed_expanded_syntax_unit(
        &mut self,
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source: &str,
        expanded_program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    ) -> Result<SyntaxUnitInput, SemanticError> {
        match self.current_expanded_syntax_unit(project, unit, generation, source, expanded_program)? {
            ExpandedRegistration::Current(input) => Ok(input),
            ExpandedRegistration::Changed { source_fingerprint, tree_fingerprint } => {
                let input = self.register_expanded_syntax(
                    project,
                    unit,
                    generation,
                    source_fingerprint,
                    tree_fingerprint,
                    Arc::new(expanded_program.clone()),
                    true,
                )?;
                self.ensure_file_text(unit.path(self).clone(), source.to_owned());
                Ok(input)
            }
        }
    }

    /// The registered input when it already describes `expanded_program` for this generation and
    /// source, else the fingerprints a new registration needs. A tree equal to the registered one
    /// (spans included) is current without computing its structural fingerprint.
    fn current_expanded_syntax_unit(
        &self,
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source: &str,
        expanded_program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    ) -> Result<ExpandedRegistration, SemanticError> {
        let source_fingerprint = Arc::<str>::from(beskid_artifacts::content_fingerprint(source));
        let existing = self.syntax_unit(unit);
        let mut same_source = None;
        if let Some(input) = existing {
            self.validate_existing_registration(input, project, generation, &source_fingerprint, true)?;
            if input.generation(self) == generation && input.source_fingerprint(self) == &source_fingerprint {
                let revision = input.revision(self);
                if std::ptr::eq(revision.expanded_program.as_ref(), expanded_program)
                    || *revision.expanded_program == *expanded_program
                {
                    return Ok(ExpandedRegistration::Current(input));
                }
                same_source = Some((input, Arc::clone(&revision.tree_fingerprint)));
            }
        }
        let tree_fingerprint = Arc::<str>::from(expanded_syntax_fingerprint(expanded_program));
        if let Some((input, registered)) = same_source
            && registered == tree_fingerprint
        {
            return Ok(ExpandedRegistration::Current(input));
        }
        Ok(ExpandedRegistration::Changed { source_fingerprint, tree_fingerprint })
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
            self.validate_existing_registration(input, project, generation, &source_fingerprint, false)?;
            if input.source_fingerprint(self) == &source_fingerprint {
                return Ok(input);
            }
        }
        let program = self.parse_and_expand(unit, &source)?;
        let tree_fingerprint = Arc::<str>::from(expanded_syntax_fingerprint(&program));
        let input = self.register_expanded_syntax(
            project,
            unit,
            generation,
            source_fingerprint,
            tree_fingerprint,
            Arc::new(program),
            false,
        )?;
        self.ensure_file_text(unit.path(self).clone(), source);
        Ok(input)
    }

    /// Replace the expanded tree of `unit` within its current `generation` by `filled`: the same
    /// tree with omitted optional arguments appended to its calls. Only `build_typed_program`
    /// calls this, between registering the parsed assembly and returning the typed program, so no
    /// node handle of the parsed tree escapes; Salsa re-derives every fact that read this unit.
    /// A canonical SDK or Dynamic unit is proven against a fresh parse and must pass every
    /// argument, so it is never refilled.
    pub(crate) fn refill_expanded_syntax_unit(
        &mut self,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        filled: Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>>,
    ) -> Result<(), SemanticError> {
        let input =
            self.syntax_unit(unit).ok_or_else(|| SemanticError::new("a refilled source unit is not registered"))?;
        let revision = input.revision(self);
        if revision.generation != generation {
            return Err(SemanticError::new("omitted arguments are filled only within the registered generation"));
        }
        if revision.runtime_source_authority.is_some() || revision.sdk_source_authority.is_some() {
            return Err(SemanticError::new("canonical SDK and Dynamic sources must pass every argument"));
        }
        let tree_fingerprint = Arc::<str>::from(expanded_syntax_fingerprint(&filled));
        let syntax_index = Arc::new(beskid_analysis::syntax_query::SyntaxIndex::from_program(&filled, generation));
        self.syntax_index_build_count.fetch_add(1, Ordering::Relaxed);
        let mut next = revision.as_ref().clone();
        next.expanded_program = filled;
        next.syntax_index = syntax_index;
        next.tree_fingerprint = tree_fingerprint;
        input.set_revision(self).to(Arc::new(next));
        Ok(())
    }

    fn register_expanded_syntax(
        &mut self,
        project: ProjectSession,
        unit: SourceUnitId,
        generation: beskid_analysis::syntax::SyntaxGenerationId,
        source_fingerprint: Arc<str>,
        tree_fingerprint: Arc<str>,
        expanded_program: Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>>,
        allow_unchanged_relabel: bool,
    ) -> Result<SyntaxUnitInput, SemanticError> {
        let registry = Arc::clone(&self.syntax_unit_registry);
        let mut registry = registry.lock().expect("syntax unit registry");
        if let Some(input) = registry.get(&unit).copied() {
            self.validate_existing_registration(
                input,
                project,
                generation,
                &source_fingerprint,
                allow_unchanged_relabel,
            )?;
            // Generation identity, not content identity, retires node handles: every
            // `AstNodeKey` names its generation and `accepts_key` admits only the current one.
            // A changed source whose expanded tree equals an earlier one (comment-only edit,
            // undo, revert) is therefore an ordinary new generation with fresh spans.
            if generation == input.generation(self) {
                return Err(SemanticError::new("one syntax generation cannot describe multiple expanded trees"));
            }
            let syntax_index =
                Arc::new(beskid_analysis::syntax_query::SyntaxIndex::from_program(&expanded_program, generation));
            self.syntax_index_build_count.fetch_add(1, Ordering::Relaxed);
            input.set_revision(self).to(Arc::new(SyntaxUnitRevision {
                runtime_source_authority: None,
                sdk_source_authority: None,
                generation,
                expanded_program,
                syntax_index,
                source_fingerprint,
                tree_fingerprint,
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
                runtime_source_authority: None,
                sdk_source_authority: None,
                generation,
                expanded_program,
                syntax_index,
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
        allow_unchanged_relabel: bool,
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
        if !allow_unchanged_relabel && !source_changed && generation != current_generation {
            return Err(SemanticError::new("unchanged syntax cannot be relabeled with a different generation"));
        }
        Ok(())
    }

    pub(crate) fn parse_and_expand(
        &self,
        unit: SourceUnitId,
        source: &str,
    ) -> Result<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>, SemanticError> {
        self.syntax_parse_count.fetch_add(1, Ordering::Relaxed);
        let source_name = unit.path(self).display().to_string();
        // A repaired tree is never registered as the tree of these bytes (fail closed).
        let program = beskid_analysis::projects::assembly::parse_program_for_unit(
            &unit.path(self),
            source,
            beskid_analysis::projects::AssemblyRecoveryPolicy::Strict,
        )
        .map_err(|error| SemanticError::new(error.to_string()))?
        .program;
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

/// Outcome of comparing an expanded unit against its registered input.
enum ExpandedRegistration {
    Current(SyntaxUnitInput),
    Changed { source_fingerprint: Arc<str>, tree_fingerprint: Arc<str> },
}
