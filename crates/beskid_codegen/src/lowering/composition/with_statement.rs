//! Lowering of `with <scope> { body }` statements to scope-stack ABI calls.
//!
//! For Phase A this emits:
//!
//! ```text
//! composition_scope_enter(active_container, scope_id);
//! // body statements are not yet wired through HIR; the analysis-driven
//! // injection lowering will populate field inject calls here.
//! composition_scope_leave(active_container, scope_id);
//! ```
//!
//! `scope_id` is derived from the scope name via a deterministic hash so the same
//! `with foo` statement always activates the same scope; once `beskid_analysis::composition`
//! threads the `ScopeId` table through to codegen, this lowering will use that
//! table directly and the hash placeholder will be removed.

use crate::errors::CodegenError;
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::pointer_type;
use beskid_abi::{SYM_COMPOSITION_SCOPE_ENTER, SYM_COMPOSITION_SCOPE_LEAVE};
use beskid_analysis::syntax::{Spanned, WithStatement};
use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, InstBuilder, Signature, types};
use cranelift_codegen::isa::CallConv;

pub(crate) fn lower_with_statement(
    node: &Spanned<WithStatement>,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<(), CodegenError> {
    let scope_id = scope_id_from_name(&node.node.scope_name.node.name);
    // Active container is threaded through the active scope frame; for the standalone path
    // we treat `null` as "use ambient container", which the runtime will reject with
    // `ContainerError::NotActive`. The companion lowering pass that links `with` to its
    // enclosing `launch` will swap this for the real container handle.
    let container = ctx.builder.ins().iconst(pointer_type(), 0);

    emit_scope(ctx, SYM_COMPOSITION_SCOPE_ENTER, container, scope_id);
    // body lowering: WithStatement body is still a syntax::Block (no HIR yet), so we
    // intentionally emit only the bracket; analysis-driven inject sites are emitted by a
    // sibling pass that consumes the composition snapshot.
    emit_scope(ctx, SYM_COMPOSITION_SCOPE_LEAVE, container, scope_id);
    Ok(())
}

fn emit_scope(
    ctx: &mut NodeLoweringContext<'_, '_>,
    symbol: &'static str,
    container: cranelift_codegen::ir::Value,
    scope_id: u32,
) {
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(pointer_type()));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I32));
    let sig_ref = ctx.builder.func.import_signature(sig);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(symbol),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    });
    let scope_arg = ctx.builder.ins().iconst(types::I64, scope_id as i64);
    ctx.builder.ins().call(func_ref, &[container, scope_arg]);
}

/// Stable hash of a scope name so codegen and tests can predict the emitted ABI argument
/// without round-tripping through `beskid_analysis::composition::ScopeId`. Will be replaced
/// by the snapshot-derived id once the analysis-to-codegen handoff lands.
pub fn scope_id_from_name(name: &str) -> u32 {
    // FNV-1a 32-bit
    let mut hash: u32 = 0x811c9dc5;
    for byte in name.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    // Reserve 0 for the global scope (matches ScopeId::GLOBAL); if a name happens to
    // hash to 0, bump it to 1 to keep the contract.
    if hash == 0 { 1 } else { hash }
}
