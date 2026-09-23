use crate::resolve::ResolvedValue;
use crate::syntax::{AstNodeId, CallExpression, Expression, Spanned};
use crate::types::TypeInfo;
use crate::types::path_value::{
    field_type_on_receiver, first_field_segment_name, method_name_from_path_callee, named_item_id,
    receiver_type_for_path_callee, resolve_path_base_local,
};
use crate::types::result::{CallLoweringKind, MethodReceiverSource};

use super::state::PrepWalker;

impl<'a> PrepWalker<'a> {
    pub(super) fn prep_call(&mut self, call_id: AstNodeId, call: &Spanned<CallExpression>) {
        if let Some(kind) = self.event_call_kind(&call.node.callee) {
            self.record_call_kind(call_id, kind);
            return;
        }

        if let Expression::Path(path) = &call.node.callee.node {
            let segs = &path.node.path.node.segments;
            let src = self.current_source_path.as_ref();
            if segs.len() >= 2
                && let Some(method) = method_name_from_path_callee(segs)
                && let Some((local, recv)) = receiver_type_for_path_callee(
                    self.resolution,
                    &self.surfaces.path_env(),
                    path.node.path.span,
                    segs,
                    src,
                )
            {
                if let Some(mid) = self.method_item_for_receiver(recv, method)
                    && let Some(sig) = self.method_dispatch_signature(mid, recv)
                {
                    self.record_call_kind(
                        call_id,
                        CallLoweringKind::MethodDispatch {
                            method_item_id: mid,
                            receiver_source: MethodReceiverSource::Local(local),
                            receiver_type: recv,
                        },
                    );
                    self.prep_arg_casts(&call.node.args, &sig.params);
                    return;
                }
                if let Some(cid) = named_item_id(&self.surfaces.path_env(), recv)
                    && let Some(sig) = self.surfaces.contract_signatures.get(&(cid, method.to_string()))
                {
                    self.record_call_kind(
                        call_id,
                        CallLoweringKind::ContractDispatch {
                            contract_item_id: cid,
                            receiver_source: MethodReceiverSource::Local(local),
                            receiver_type: recv,
                        },
                    );
                    self.prep_arg_casts(&call.node.args, &sig.params);
                    return;
                }
            }
            if segs.len() >= 2
                && let Some(ResolvedValue::Item(cid)) = self.resolved_value_at(path.node.path.span)
                && let Some(method) = method_name_from_path_callee(segs)
                && let Some(sig) = self.surfaces.contract_signatures.get(&(cid, method.to_string()))
                && let Some(recv) = self.named_type_id(cid)
            {
                self.record_call_kind(
                    call_id,
                    CallLoweringKind::ContractDispatch {
                        contract_item_id: cid,
                        receiver_source: MethodReceiverSource::Expression(path.node.path.span),
                        receiver_type: recv,
                    },
                );
                self.prep_arg_casts(&call.node.args, &sig.params);
                return;
            }
        }

        if let Expression::Member(mem) = &call.node.callee.node {
            if let Expression::Path(path) = &mem.node.target.node
                && let Some(ResolvedValue::Item(cid)) = self.resolved_value_at(path.node.path.span)
                && let Some(sig) =
                    self.surfaces.contract_signatures.get(&(cid, mem.node.member.node.name.as_str().to_string()))
                && let Some(recv) = self.named_type_id(cid)
            {
                self.record_call_kind(
                    call_id,
                    CallLoweringKind::ContractDispatch {
                        contract_item_id: cid,
                        receiver_source: MethodReceiverSource::Expression(mem.node.target.span),
                        receiver_type: recv,
                    },
                );
                self.prep_arg_casts(&call.node.args, &sig.params);
                return;
            }
            if let Some(recv) = self.expr_type(&mem.node.target) {
                let method = mem.node.member.node.name.as_str();
                if let Some(mid) = self.method_item_for_receiver(recv, method)
                    && let Some(sig) = self.method_dispatch_signature(mid, recv)
                {
                    self.record_call_kind(
                        call_id,
                        CallLoweringKind::MethodDispatch {
                            method_item_id: mid,
                            receiver_source: MethodReceiverSource::Expression(mem.node.target.span),
                            receiver_type: recv,
                        },
                    );
                    self.prep_arg_casts(&call.node.args, &sig.params);
                    return;
                }
                if let Some(cid) = named_item_id(&self.surfaces.path_env(), recv)
                    && let Some(sig) = self.surfaces.contract_signatures.get(&(cid, method.to_string()))
                {
                    self.record_call_kind(
                        call_id,
                        CallLoweringKind::ContractDispatch {
                            contract_item_id: cid,
                            receiver_source: MethodReceiverSource::Expression(mem.node.target.span),
                            receiver_type: recv,
                        },
                    );
                    self.prep_arg_casts(&call.node.args, &sig.params);
                    return;
                }
            }
        }

        let item_callee = matches!(&call.node.callee.node, Expression::Path(p)
            if matches!(self.resolved_value_at(p.node.path.span), Some(ResolvedValue::Item(_))));
        if !item_callee
            && let Some(ct) = self.expr_type(&call.node.callee)
            && let Some(TypeInfo::Function { params, .. }) = self.surfaces.types.get(ct)
        {
            self.record_call_kind(call_id, CallLoweringKind::CallableValueCall);
            self.prep_arg_casts(&call.node.args, params);
            return;
        }

        if let Expression::Path(p) = &call.node.callee.node
            && let Some(ResolvedValue::Item(id)) = self.resolved_value_at(p.node.path.span)
        {
            self.record_call_kind(call_id, CallLoweringKind::ItemCall { item_id: id });
            if let Some(sig) = self.surfaces.function_signatures.get(&id) {
                self.prep_arg_casts(&call.node.args, &sig.params);
            }
        }
    }

    pub(super) fn event_call_kind(&self, callee: &Spanned<Expression>) -> Option<CallLoweringKind> {
        let (src, recv, item, field) = match &callee.node {
            Expression::Member(m) => {
                let recv = self.expr_type(&m.node.target)?;
                let item = named_item_id(&self.surfaces.path_env(), recv)?;
                (
                    MethodReceiverSource::Expression(m.node.target.span),
                    recv,
                    item,
                    m.node.member.node.name.as_str().to_string(),
                )
            }
            Expression::Path(p) => {
                let segs = &p.node.path.node.segments;
                let field = first_field_segment_name(segs)?.to_string();
                let first = segs.first()?.node.name.node.name.as_str();
                let local = resolve_path_base_local(
                    self.resolution,
                    p.node.path.span,
                    first,
                    self.current_source_path.as_ref(),
                )?;
                let recv = self.surfaces.local_types.get(&local).copied()?;
                let item = named_item_id(&self.surfaces.path_env(), recv)?;
                (MethodReceiverSource::Local(local), recv, item, field)
            }
            _ => return None,
        };
        self.surfaces.struct_event_fields.get(&item).and_then(|f| f.get(&field))?;
        field_type_on_receiver(
            self.resolution,
            &self.surfaces.path_env(),
            recv,
            &field,
            self.current_source_path.as_ref(),
        )?;
        Some(CallLoweringKind::EventInvoke { receiver_source: src, receiver_type: recv })
    }
}
