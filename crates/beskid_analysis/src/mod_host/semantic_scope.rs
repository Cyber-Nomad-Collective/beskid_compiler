//! Synchronous invocation scopes never store or thread a borrowed semantic owner.
use super::{ModHostInput, ModSemanticAuthority};
use crate::{
    projects::ProgramAssembly,
    syntax::{Program, Spanned},
};
use anyhow::{Result, bail};

/// The query owner registers the exact current program before invoking the callback.
/// A scope must invoke once and drop its borrowed authority before returning the assembly.
pub trait ModSemanticScope {
    fn with_authority(
        &self,
        program: Option<&Spanned<Program>>,
        invoke: &mut dyn FnMut(&dyn ModSemanticAuthority) -> Result<()>,
    ) -> Result<ProgramAssembly>;
}

pub(crate) struct ScopedInvocation<T> {
    pub outcome: T,
    pub issued_generation: Option<crate::syntax::SyntaxGenerationId>,
    pub assembly: Option<ProgramAssembly>,
}

pub(crate) fn invoke_in_scope<T>(
    input: &ModHostInput<'_>,
    program: Option<&Spanned<Program>>,
    action: impl FnOnce(&ModHostInput<'_>) -> Result<T>,
) -> Result<ScopedInvocation<T>> {
    let Some(scope) = input.semantic_scope else {
        return Ok(ScopedInvocation {
            outcome: action(input)?,
            issued_generation: input.syntax_generation_id,
            assembly: None,
        });
    };
    let mut action = Some(action);
    let mut outcome = None;
    let mut issued_generation = None;
    let assembly = scope.with_authority(program, &mut |authority| {
        let action = action.take().ok_or_else(|| anyhow::anyhow!("Mod scope attempted repeated invocation"))?;
        issued_generation = Some(authority.generation());
        let scoped = ModHostInput {
            semantic_scope: None,
            semantic_authority: Some(authority),
            syntax_generation_id: Some(authority.generation()),
            compile_plan: input.compile_plan,
            source_name: input.source_name,
            source: input.source,
            pipeline: input.pipeline,
            invoker: input.invoker,
            cached_target_fingerprint: input.cached_target_fingerprint,
        };
        outcome = Some(action(&scoped)?);
        Ok(())
    })?;
    let Some(outcome) = outcome else {
        bail!("Mod semantic scope returned without invoking its callback");
    };
    Ok(ScopedInvocation { outcome, issued_generation, assembly: Some(assembly) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_host::{ModSemanticDeclaration, ModSemanticError, ModSemanticHandle, ModSemanticShape};
    use crate::syntax::SyntaxGenerationId;
    struct Authority;
    impl crate::mod_host::ModSyntaxAuthority for Authority {
        fn take_applied_syntax(
            &self,
            _issuer: u64,
        ) -> Result<Vec<crate::mod_host::ModAppliedSyntax>, crate::mod_host::ModSemanticError> {
            Ok(Vec::new())
        }
        fn syntax_query(
            &self,
            _issuer: u64,
            _request: &crate::mod_host::ModSyntaxRequest,
        ) -> Result<crate::mod_host::ModSyntaxResponse, crate::mod_host::ModSemanticError> {
            Err(crate::mod_host::ModSemanticError("test authority has no registered syntax assembly".into()))
        }
    }
    impl ModSemanticAuthority for Authority {
        fn generation(&self) -> SyntaxGenerationId {
            SyntaxGenerationId(91)
        }
        fn resolve_declaration(&self, _: &ModSemanticDeclaration) -> Result<ModSemanticHandle, ModSemanticError> {
            Err(ModSemanticError("test authority has no declarations".into()))
        }
        fn type_shape(&self, _: ModSemanticHandle) -> Result<ModSemanticShape, ModSemanticError> {
            Err(ModSemanticError("test authority has no shapes".into()))
        }
    }
    struct RepeatedScope;
    impl ModSemanticScope for RepeatedScope {
        fn with_authority(
            &self,
            _: Option<&Spanned<Program>>,
            invoke: &mut dyn FnMut(&dyn ModSemanticAuthority) -> Result<()>,
        ) -> Result<ProgramAssembly> {
            invoke(&Authority)?;
            invoke(&Authority)?;
            bail!("test scope never produces an assembly")
        }
    }
    #[test]
    fn repeated_scope_cannot_invoke_contract_twice_or_retain_action() {
        let scope = RepeatedScope;
        let input = ModHostInput { semantic_scope: Some(&scope), ..Default::default() };
        let mut calls = 0;
        let result = invoke_in_scope(&input, None, |scoped| {
            calls += 1;
            assert!(scoped.semantic_scope.is_none());
            assert_eq!(scoped.semantic_authority.unwrap().generation(), SyntaxGenerationId(91));
            assert_eq!(scoped.syntax_generation_id, Some(SyntaxGenerationId(91)));
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }
    #[test]
    fn diagnostic_only_input_retains_borrowed_source_without_creating_authority() {
        let input = ModHostInput {
            source: "source bytes",
            source_name: "Main.bd",
            syntax_generation_id: Some(SyntaxGenerationId(7)),
            ..Default::default()
        };
        let result = invoke_in_scope(&input, None, |scoped| {
            assert!(scoped.semantic_authority.is_none());
            Ok((scoped.source.to_owned(), scoped.source_name.to_owned()))
        })
        .unwrap();
        assert_eq!(result.outcome, ("source bytes".into(), "Main.bd".into()));
        assert!(result.assembly.is_none());
        assert_eq!(result.issued_generation, Some(SyntaxGenerationId(7)));
    }
}
