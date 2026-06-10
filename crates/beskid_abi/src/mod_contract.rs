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

/// Analyzer request payload (`Beskid.Compiler.Collect.AnalysisRequest`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModAnalysisRequest {
    pub context: ModCollectRequest,
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
