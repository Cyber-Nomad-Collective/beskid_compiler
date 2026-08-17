//! C-layout descriptors for compiler-mod contract entrypoints (native host bridge).
//!
//! Layout mirrors `Beskid.Compiler.*` SDK types until full Beskid-side marshaling
//! is wired. The host reads `[InternalSymbol]` metadata to validate parameter schemas.

use std::os::raw::c_void;

use crate::types::BeskidStr;

/// Slice header for `string[]` fields in mod context structs.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModStrSlice {
    pub items: *const BeskidStr,
    pub len: usize,
}

/// Active host compilation summary (`Beskid.Compiler.Compilation`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModCompilation {
    pub active_project_name: BeskidStr,
    pub active_project_root: BeskidStr,
    pub target_triple: BeskidStr,
    pub syntax_generation_id: u64,
    pub entry_source_path: BeskidStr,
    pub entry_source_name: BeskidStr,
    /// Entry source text the host already holds in memory. Lets native `Analyzer` /
    /// `Rewriter` contracts read the entry source without disk I/O.
    pub entry_source_text: BeskidStr,
}

/// One workspace member (`Beskid.Compiler.Workspace.WorkspaceMember`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModWorkspaceMember {
    pub member_id: BeskidStr,
    pub project_name: BeskidStr,
    pub project_root: BeskidStr,
    pub source_root: BeskidStr,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModWorkspaceMemberSlice {
    pub items: *const ModWorkspaceMember,
    pub len: usize,
}

/// Workspace summary (`Beskid.Compiler.Workspace`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModWorkspace {
    pub root_path: BeskidStr,
    pub members: ModWorkspaceMemberSlice,
    pub lock_hash: BeskidStr,
}

/// One mod contract registration (`Beskid.Compiler.ModPackage.ModContractRegistration`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModContractRegistration {
    pub contract_id: BeskidStr,
    pub type_id: BeskidStr,
    pub entry_symbol: BeskidStr,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModContractRegistrationSlice {
    pub items: *const ModContractRegistration,
    pub len: usize,
}

/// Single loaded mod package (`Beskid.Compiler.ModPackage`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModPackage {
    pub package_id: BeskidStr,
    pub package_version: BeskidStr,
    pub project_name: BeskidStr,
    pub project_root: BeskidStr,
    pub source_root: BeskidStr,
    pub manifest_path: BeskidStr,
    pub descriptor_path: BeskidStr,
    pub capabilities: ModStrSlice,
    pub registrations: ModContractRegistrationSlice,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModPackageSlice {
    pub items: *const ModPackage,
    pub len: usize,
}

/// Loaded mod catalog (`Beskid.Compiler.ModCatalog`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModCatalog {
    pub packages: ModPackageSlice,
}

/// Collector / Analyzer shared context (`Beskid.Compiler.Collect.CollectRequest`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModCollectRequest {
    pub compilation: ModCompilation,
    pub workspace: ModWorkspace,
    pub mods: ModCatalog,
}

/// Collector target set (`Beskid.Compiler.Collect.CollectTargetSet`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModCollectTargetSet {
    pub target_ids: ModStrSlice,
}

/// Generator request payload (`Beskid.Compiler.Collect.GenerationRequest`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModGenerationRequest {
    pub context: ModCollectRequest,
    pub targets: ModCollectTargetSet,
}

/// Opaque handle to a host-side semantic query surface. Phase 1 (Option C) forwards a
/// null `ptr` with `version: 0` — mods re-derive diagnostics from source text + the
/// syntax tree. Phase 2 (Option A) populates `ptr` with a callback vtable and bumps
/// `version`; the struct layout is unchanged so native artifacts compiled against
/// Phase 1 keep working (they see `version: 0` and ignore `ptr`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModSemanticHandle {
    /// Null in Phase 1; callback vtable pointer in Phase 2.
    pub ptr: *const c_void,
    /// 0 = no semantic surface; 1 = vtable v1.
    pub version: u32,
}

impl ModSemanticHandle {
    /// Phase 1 null handle — no semantic query surface.
    pub const fn null() -> Self {
        Self { ptr: std::ptr::null(), version: 0 }
    }
}

/// Analyzer request payload (`Beskid.Compiler.Collect.AnalysisRequest`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModAnalysisRequest {
    pub context: ModCollectRequest,
    /// Forward-compatible semantic query handle. Null in Phase 1 (Option C).
    pub semantic: ModSemanticHandle,
}

/// Tagged union tag for [`ModSyntaxContributionItem`].
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSyntaxContributionTag {
    ContractDefinition = 0,
    TypeDefinition = 1,
    FunctionDefinition = 2,
}

/// Opaque pointer to a host-owned syntax node tree (materialized by emit_bridge).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModSyntaxNodeHandle {
    pub ptr: *const c_void,
}

/// One top-level item returned from `Generator.Generate`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModSyntaxContributionItem {
    pub tag: ModSyntaxContributionTag,
    pub node: ModSyntaxNodeHandle,
}

/// Slice header for typed generator output (`GeneratedSyntaxContribution.items`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModSyntaxContributionSlice {
    pub items: *const ModSyntaxContributionItem,
    pub len: usize,
}

/// Return payload from a native `Generator.Generate` entrypoint.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModGeneratedSyntaxContribution {
    pub items: ModSyntaxContributionSlice,
}

/// Native callable signature: `entry_symbol(*const ModGenerationRequest) -> *const ModGeneratedSyntaxContribution`.
pub type ModGeneratorEntryFn =
    unsafe extern "C" fn(*const ModGenerationRequest) -> *const ModGeneratedSyntaxContribution;

/// Native callable: `entry_symbol(*const ModCollectRequest) -> *const ModCollectTargetSet`.
pub type ModCollectorEntryFn = unsafe extern "C" fn(*const ModCollectRequest) -> *const ModCollectTargetSet;

/// Native callable: `entry_symbol(*const ModAnalysisRequest) -> *const ModAnalysisResult`.
pub type ModAnalyzerEntryFn = unsafe extern "C" fn(*const ModAnalysisRequest) -> *const ModAnalysisResult;

/// Native callable: `entry_symbol(*const ModCollectRequest) -> *const ModRewriteResult`.
pub type ModRewriterEntryFn = unsafe extern "C" fn(*const ModCollectRequest) -> *const ModRewriteResult;

/// One diagnostic from a native analyzer.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModDiagnostic {
    pub code: BeskidStr,
    pub message: BeskidStr,
    /// 0 = Error, 1 = Warning, 2 = Note.
    pub severity: u32,
    /// Inclusive byte offset range in the entry source. `span_end == span_start` for
    /// point diagnostics; both are clamped to the source length by the host.
    pub span_start: u64,
    pub span_end: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModDiagnosticSlice {
    pub items: *const ModDiagnostic,
    pub len: usize,
}

/// One quick-fix produced by a native `Analyzer`. `diagnostic_index` links the fix to an
/// entry in the same `ModAnalysisResult.diagnostics` slice so the host can resolve the
/// target diagnostic without string matching (codes can collide across mods).
///
/// Reuses [`ModEdit`] (`mod_contract.rs:213-224`) for the edit payload — its
/// Insert/Replace/Delete shape is exactly what a quick-fix needs.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModQuickFix {
    /// Indexes into `ModAnalysisResult.diagnostics`.
    pub diagnostic_index: u32,
    pub title: BeskidStr,
    pub edits: ModEditSlice,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModQuickFixSlice {
    pub items: *const ModQuickFix,
    pub len: usize,
}

/// Result from `Analyzer.Analyze`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModAnalysisResult {
    pub diagnostics: ModDiagnosticSlice,
    /// Flat list of quick-fixes; each carries a `diagnostic_index` into `diagnostics`.
    pub fixes: ModQuickFixSlice,
}

/// One text edit from a native rewriter.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModEdit {
    /// 0 = Insert, 1 = Replace, 2 = Delete.
    pub kind: u32,
    /// Byte offset where the edit begins. For `Insert`, equal to `end`.
    pub start: u64,
    /// Byte offset where the edit ends (exclusive). For `Insert`, equal to `start`.
    pub end: u64,
    /// Replacement / inserted text. Empty for `Delete`.
    pub text: BeskidStr,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModEditSlice {
    pub items: *const ModEdit,
    pub len: usize,
}

/// Result from `Rewriter.Rewrite`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModRewriteResult {
    pub edits: ModEditSlice,
}
