//! Focused regression coverage for `production_isa_settings_builder`'s inline stack-probe
//! setting (`crates/beskid_codegen/src/cranelift_host.rs`).
//!
//! Cranelift's default settings (`cranelift-codegen` 0.136.0, `src/settings.rs`) are
//! `enable_probestack = false`, `probestack_strategy = "outline"`, `probestack_size_log2 = 12`
//! (a 4 KiB guard, matching the OS page/guard-page size). Left at the default, a single function
//! whose own stack frame exceeds one guard page can `sub rsp, N` straight past the guard region
//! without ever touching it (a "stack clash"), landing in genuinely unmapped memory instead of
//! faulting cleanly on the guard page. This is independent of recursion depth -- it only takes
//! one oversized frame.
//!
//! `production_isa_settings_builder` now sets `enable_probestack = true` and
//! `probestack_strategy = "inline"`, which makes Cranelift emit the probe directly in the
//! function's own prologue (`gen_inline_probestack`, `cranelift-codegen`
//! `src/isa/x64/abi.rs:628-650`): a store to each guard-sized page from the frame's top down to
//! `rsp`, unrolled for small frames (<= 4 probes) or `stack_probe_loop` for larger ones -- no
//! external symbol required (unlike `probestack_strategy = "outline"`, which would call
//! `ExternalName::LibCall(LibCall::Probestack)` and require the JIT and AOT link paths to both
//! resolve a `__cranelift_probestack`-equivalent host export that does not exist in this runtime
//! today).
//!
//! This test builds a raw Cranelift function (no Beskid source needed) with one explicit stack
//! slot large enough to need several guard-page probes, compiles it for x86_64 with the exact
//! shared settings builder every JIT/AOT backend in this codebase uses, and asserts the compiled
//! prologue actually contains the `stack_probe_loop` marker Cranelift's own x64 instruction
//! printer uses for it (`cranelift-codegen` `src/isa/x64/inst/mod.rs:578`). A control build with
//! probing forced back off proves the marker is specific to the setting, not always present.
//!
//! DRAFTED, NOT YET RUN: written while the remote builder (the only place this workspace is
//! allowed to `cargo build`/`cargo test`, per `~/.claude/handoffs/v05-agent-rules.md`) was
//! offline. Needs a real `cargo test -p beskid_codegen --test stack_probe_inline` pass on the
//! builder before this can be treated as verified.

use cranelift_codegen::Context;
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, StackSlotData, StackSlotKind, UserFuncName, types};
use cranelift_codegen::isa::{self, CallConv};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

/// One explicit stack slot large enough to need more than the 4-probe inline-unroll threshold
/// (`PROBE_MAX_UNROLL = 4` in `gen_inline_probestack`) at the default 4 KiB guard size, so the
/// loop form (`stack_probe_loop`) is exercised rather than the unrolled form.
const LARGE_FRAME_SLOT_BYTES: u32 = 20 * 1024;

fn x86_64_isa(flags: settings::Flags) -> std::sync::Arc<dyn isa::TargetIsa> {
    isa::lookup_by_name("x86_64").expect("x86_64 ISA is always available for cross-compilation").finish(flags).expect("ISA settings are valid")
}

/// Builds `fn() -> i64` that reads through a pointer-sized local stored at the far end of a
/// `LARGE_FRAME_SLOT_BYTES` explicit stack slot -- large enough on its own to force multiple
/// guard-page probes, mirroring what a Beskid function with a large local array/struct would
/// need (not what `RecurseHoldingLocals` itself needs -- that function's per-frame footprint is
/// far under one guard page; this isolates the *single-oversized-frame* stack-clash gap from the
/// *cumulative-recursion* question the `heap_growth_native.rs::deep_recursion_*` test covers).
fn build_large_frame_function(isa: &dyn isa::TargetIsa) -> Function {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.returns.push(AbiParam::new(types::I64));
    let mut function = Function::with_name_signature(UserFuncName::testcase("large_frame_probe_target"), signature);
    let mut builder_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
    let block = builder.create_block();
    builder.switch_to_block(block);
    let slot = builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, LARGE_FRAME_SLOT_BYTES, 0));
    let zero = builder.ins().iconst(types::I64, 0);
    // `stack_store` takes the pointer type explicitly as its first operand in this Cranelift
    // version (matches the real usage in
    // `crates/beskid_isle/src/context/roots.rs::new_root_slot`). Storing into the slot (instead of
    // leaving it dead) keeps the frame from being optimized away trivially; we return the constant
    // directly rather than reading back through `stack_addr`/`load`, which avoids needing a
    // `MemFlags` value this synthetic function has no real safety story for.
    builder.ins().stack_store(types::I64, zero, slot, (LARGE_FRAME_SLOT_BYTES - 8) as i32);
    builder.ins().return_(&[zero]);
    builder.seal_all_blocks();
    builder.finalize(isa.frontend_config());
    function
}

fn compiled_vcode(isa: &dyn isa::TargetIsa, function: Function) -> String {
    let mut context = Context::for_function(function);
    context.set_disasm(true);
    let mut control_plane = ControlPlane::default();
    let compiled = context.compile(isa, &mut control_plane).expect("large-frame function compiles under every ISA setting combination tested here");
    compiled.vcode.clone().expect("set_disasm(true) requests vcode text")
}

#[test]
fn inline_probestack_setting_emits_a_stack_probe_loop_for_an_oversized_frame() {
    let flags = settings::Flags::new(
        beskid_codegen::cranelift_host::production_isa_settings_builder().expect("production ISA settings build cleanly"),
    );
    assert!(flags.enable_probestack(), "production_isa_settings_builder must enable stack probing");
    let isa = x86_64_isa(flags);
    let vcode = compiled_vcode(isa.as_ref(), build_large_frame_function(isa.as_ref()));
    assert!(
        vcode.contains("stack_probe_loop"),
        "a {LARGE_FRAME_SLOT_BYTES}-byte frame (several guard pages past the default 4 KiB \
         probestack_size_log2=12 threshold) must probe every intervening page inline instead of \
         `sub rsp` past them in one instruction; got vcode:\n{vcode}"
    );
}

#[test]
fn disabling_probestack_removes_the_probe_from_the_same_oversized_frame() {
    // Differential control: proves the marker above is actually gated by the setting, not just
    // always present in every compiled prologue regardless of configuration.
    let mut builder = settings::builder();
    builder.set("preserve_frame_pointers", "true").unwrap();
    builder.set("enable_probestack", "false").unwrap();
    let flags = settings::Flags::new(builder);
    let isa = x86_64_isa(flags);
    let vcode = compiled_vcode(isa.as_ref(), build_large_frame_function(isa.as_ref()));
    assert!(
        !vcode.contains("stack_probe_loop"),
        "enable_probestack=false must not emit a stack probe, confirming the marker checked \
         above is meaningful and not incidental:\n{vcode}"
    );
}
