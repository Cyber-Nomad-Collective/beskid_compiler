//! Invocation-local registration and borrowed semantic projection for compiler Mods.
use crate::{BeskidDatabase, ModSemanticQueryAuthority};
use anyhow::{Result, bail};
use beskid_analysis::{
    mod_host::{ModSemanticAuthority, ModSemanticScope},
    projects::ProgramAssembly,
    services::ResolvedInput,
    syntax::{Program, Spanned, SyntaxGenerationId},
};
use std::cell::RefCell;
use std::sync::Arc;

/// Register the program a Mod invocation's semantic authority serves, with the same source
/// authority as the compilation (`entry.rs` fact authority, `beskid_codegen::prepared_syntax`):
/// compiler-owned Corelib units are attested through the canonical service capability, so
/// canonical path planning finds their declarations. A bare typed program attests none, and every
/// planned route (first `Core.Results.Result`) would be absent.
pub fn build_mod_authority_typed_program(
    db: &mut BeskidDatabase,
    project: crate::ProjectSession,
    assembly: Arc<ProgramAssembly>,
) -> Result<crate::TypedProgram> {
    let authority_target = beskid_abi::runtime_kit::host_runtime_target()
        .map_err(|error| anyhow::anyhow!("host ABI-v5 target unavailable for Mod source authority: {error}"))?;
    let manifest = beskid_abi::abi_v5::AbiManifestV5::canonical_runtime(authority_target);
    let generation = assembly.generation;
    Ok(if assembly.runtime_fixture.is_some() {
        crate::build_runtime_fixture_typed_program(db, project, generation, assembly, &manifest)?
    } else {
        let capability = beskid_abi::runtime_source::canonical_corelib_syscall_service_capability(&manifest)
            .map_err(|error| anyhow::anyhow!("canonical Corelib service authority unavailable: {error:?}"))?;
        crate::build_typed_program_with_corelib_syscall_services(db, project, generation, assembly, capability)?
    })
}

pub(crate) struct QueryModSemanticScope<'a> {
    db: RefCell<&'a mut BeskidDatabase>,
    planned: Option<&'a ResolvedInput>,
    assembly: RefCell<ProgramAssembly>,
}
impl<'a> QueryModSemanticScope<'a> {
    pub(crate) fn new(
        db: &'a mut BeskidDatabase,
        planned: Option<&'a ResolvedInput>,
        assembly: ProgramAssembly,
    ) -> Self {
        Self { db: RefCell::new(db), planned, assembly: RefCell::new(assembly) }
    }
    pub(crate) fn assembly(&self) -> ProgramAssembly {
        self.assembly.borrow().clone()
    }
    pub(crate) fn with_db<T>(&self, action: impl FnOnce(&mut BeskidDatabase) -> Result<T>) -> Result<T> {
        let mut db = self.db.try_borrow_mut().map_err(|_| anyhow::anyhow!("reentrant Mod query authority access"))?;
        action(&mut db)
    }
    fn update_program(&self, program: Option<&Spanned<Program>>) -> Result<()> {
        let Some(program) = program else {
            return Ok(());
        };
        let mut current =
            self.assembly.try_borrow_mut().map_err(|_| anyhow::anyhow!("reentrant Mod assembly update"))?;
        if current.entry_unit().program == *program {
            return Ok(());
        }
        current.generation.resume_after();
        let generation =
            SyntaxGenerationId::allocate().ok_or_else(|| anyhow::anyhow!("syntax generation exhausted"))?;
        let mut units = current.units.as_ref().clone();
        units[current.entry_index].program = program.clone();
        let mut next = ProgramAssembly::new(
            current.roots.clone(),
            Arc::new(units),
            current.entry_index,
            current.discovery,
            Arc::clone(&current.module_index),
            current.has_core_dependency,
            generation,
        )
        .with_recovery_policy(current.recovery_policy)
        .with_trusted_corelib_service_paths(Arc::clone(&current.trusted_corelib_service_paths))
        .with_glue_libraries(Arc::clone(&current.glue_libraries))
        .with_runtime_fixture(current.runtime_fixture.clone())
        .with_root_set(current.root_set.clone());
        next.verified_package_identities = current.package_identities().clone();
        next.compiled_mod_metadata = current.compiled_mod_metadata.clone();
        *current = next;
        Ok(())
    }
}
impl ModSemanticScope for QueryModSemanticScope<'_> {
    fn with_authority(
        &self,
        program: Option<&Spanned<Program>>,
        invoke: &mut dyn FnMut(&dyn ModSemanticAuthority) -> Result<()>,
    ) -> Result<ProgramAssembly> {
        self.update_program(program)?;
        let assembly = self.assembly.borrow().clone();
        if assembly.units.is_empty() {
            bail!("Mod semantic scope has no registered source units");
        }
        let registered = Arc::new(assembly.clone());
        let mut db = self.db.try_borrow_mut().map_err(|_| anyhow::anyhow!("reentrant Mod query authority access"))?;
        let project = match self.planned.and_then(|resolved| Some((resolved.compile_plan.as_ref()?, resolved))) {
            Some((plan, resolved)) => crate::project_session_for_planned_syntax_assembly(
                &mut db,
                &registered,
                plan,
                &resolved.source_path,
                crate::typed_entry_bundle::lockfile_digest_for_plan(plan),
            )?,
            None => {
                crate::project_session_for_syntax_assembly(&mut db, &registered, "mod-invocation", "source-authority")?
            }
        };
        build_mod_authority_typed_program(&mut db, project, Arc::clone(&registered))?;
        let authority = ModSemanticQueryAuthority::for_registered_assembly(&**db, project, &registered)?;
        invoke(&authority)?;
        let applied = authority.take_all_applied_syntax();
        let issued = authority.take_issued_metadata();
        // Borrowed issuer map and database borrow both end before the next source generation.
        drop(authority);
        drop(db);
        if !issued.is_empty() {
            let mut current =
                self.assembly.try_borrow_mut().map_err(|_| anyhow::anyhow!("reentrant Mod assembly update"))?;
            for metadata in issued {
                if !current.compiled_mod_metadata.contains(&metadata) {
                    current.compiled_mod_metadata.push(metadata);
                }
            }
        }
        let mut changed = None;
        for update in applied {
            if update.previous_generation != assembly.generation
                || update.source_unit != assembly.entry_unit().path
                || changed.is_some()
            {
                bail!("conflicting or foreign applied syntax handoff");
            }
            changed = Some(update.program);
        }
        if let Some(program) = changed {
            self.update_program(Some(&program))?;
        }
        Ok(self.assembly.borrow().clone())
    }
}

impl beskid_analysis::services::SemanticFactAuthority for QueryModSemanticScope<'_> {
    fn fact_findings(
        &mut self,
        assembly: &ProgramAssembly,
        program: &Spanned<Program>,
        pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
    ) -> Result<Vec<beskid_analysis::services::SemanticFactFinding>> {
        self.update_program(Some(program))?;
        let current = self.assembly();
        if current.generation != assembly.generation {
            bail!("semantic findings assembly differs from current Mod generation");
        }
        self.with_db(|db| crate::entry::semantic_diagnostics_for_roots(db, self.planned, &current, program, pipeline))
    }
    fn mod_scope(&self) -> Option<&dyn ModSemanticScope> {
        Some(self)
    }
}
