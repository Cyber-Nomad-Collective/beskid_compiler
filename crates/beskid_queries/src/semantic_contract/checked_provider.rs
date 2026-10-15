//! Reviewed synchronous canonical provider leaves, distinct from callable ownership.
use super::*;
use beskid_analysis::syntax_query::NodeKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProviderCall {
    call: AstNodeKey,
    symbol: &'static str,
}
impl CheckedProviderCall {
    pub fn call(&self) -> AstNodeKey {
        self.call
    }
    pub fn symbol(&self) -> &'static str {
        self.symbol
    }
}

/// These exact canonical leaves neither switch the managed scheduler nor invoke user code.
/// Allocating UTF8 leaves dispatch through the canonical runtime's exact source-issued
/// checked constructor closure before any payload store or publication. Runtime-kit finalization
/// requires that executable export, not only its descriptive ABI row. Source
/// capability comes from ordinary call lowering, never from the service spelling alone.
pub fn checked_provider_call(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CheckedProviderCall> {
    let Some(CallLowering::CorelibService(service)) = call_lowering(db, key)? else { return Ok(None) };
    let permitted = matches!(
        service.name,
        "__gc_same_identity"
            | "__float_to_bits32"
            | "__float_from_bits32"
            | "__float_to_bits64"
            | "__float_from_bits64"
            | "__array_len"
            | "__str_len"
            | "__str_slice"
            | "__str_from_bytes_utf8"
    );
    if !permitted {
        return Ok(None);
    }
    Ok(Some(CheckedProviderCall { call: key, symbol: service.symbol }))
}

/// Only the exact source-issued Foundation facade can call a checked result
/// dispatcher. The dispatcher itself admits only retained checked callback counterparts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedDynamicResultBridge {
    declaration: AstNodeKey,
    symbol: &'static str,
}
impl CheckedDynamicResultBridge {
    pub fn declaration(&self) -> AstNodeKey {
        self.declaration
    }
    pub fn symbol(&self) -> &'static str {
        self.symbol
    }
}
pub fn checked_dynamic_result_bridge(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CheckedDynamicResultBridge> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|unit| unit.accepts_key(db, key)) else { return Ok(None) };
    if syntax.revision(db).runtime_source_authority.as_deref()
        != Some(beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH)
        || syntax.syntax_index(db).kind(key.node) != Some(NodeKind::ContractMethodSignature)
    {
        return Ok(None);
    }
    let symbol = match item_name(db, key)?.as_deref() {
        Some("beskid_dynamic_v1_create_result") => "beskid_dynamic_v1_checked_create_result_dispatch",
        Some("beskid_dynamic_v1_cast_result") => "beskid_dynamic_v1_checked_cast_result_dispatch",
        Some("beskid_dynamic_v1_map_result") => "beskid_dynamic_v1_checked_map_result_dispatch",
        _ => return Ok(None),
    };
    Ok(Some(CheckedDynamicResultBridge { declaration: key, symbol }))
}
