use super::prelude::{AstNodeId, AstNodeKey, DirectCallee, NodeFacts, SourceUnitId, SyntaxGenerationId, types};

pub(in super::super) struct CorelibServiceImportFacts {
    pub(in super::super) call: AstNodeKey,
    fd: AstNodeKey,
    limit: AstNodeKey,
    service: DirectCallee,
}

impl CorelibServiceImportFacts {
    pub(in super::super) fn new(db: &dyn beskid_queries::Db, service: DirectCallee) -> Self {
        let unit = SourceUnitId::new(db, std::path::PathBuf::from("/tmp/CorelibService.bd"));
        let generation = SyntaxGenerationId(93);
        Self {
            call: AstNodeKey { unit, generation, node: AstNodeId(1) },
            fd: AstNodeKey { unit, generation, node: AstNodeId(2) },
            limit: AstNodeKey { unit, generation, node: AstNodeId(3) },
            service,
        }
    }
}

impl NodeFacts for CorelibServiceImportFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<beskid_isle::NodeKind> {
        (key == self.call)
            .then_some(beskid_isle::NodeKind::CallExpression)
            .or_else(|| (key == self.fd || key == self.limit).then_some(beskid_isle::NodeKind::LiteralExpression))
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<beskid_isle::LiteralKind> {
        (key == self.fd || key == self.limit).then_some(beskid_isle::LiteralKind::Integer)
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<beskid_isle::CallKind> {
        (key == self.call).then_some(beskid_isle::CallKind::Direct)
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.fd).then_some(0).or_else(|| (key == self.limit).then_some(16))
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        if key == self.call { Some(types::I64) } else { (key == self.fd || key == self.limit).then_some(types::I64) }
    }

    fn direct_callee(&self, key: AstNodeKey) -> Option<DirectCallee> {
        (key == self.call).then_some(self.service.clone())
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Signature> {
        (key == self.call).then(|| cranelift_codegen::ir::Signature {
            params: vec![
                cranelift_codegen::ir::AbiParam::new(types::I64),
                cranelift_codegen::ir::AbiParam::new(types::I64),
            ],
            returns: vec![cranelift_codegen::ir::AbiParam::new(types::I64)],
            call_conv: cranelift_codegen::isa::CallConv::SystemV,
        })
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (key == self.call).then_some(vec![self.fd, self.limit])
    }
}
