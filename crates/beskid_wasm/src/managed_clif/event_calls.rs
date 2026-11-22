use super::*;
use crate::browser_heap::events::Operation;

impl Pass<'_> {
    pub(super) fn event_call(
        &mut self,
        inst: Inst,
        operation: Operation,
        arguments: &[Value],
    ) -> Result<Edit, &'static str> {
        let Some((address, rest)) = arguments.split_first() else {
            return Err("event service lacks its owner address");
        };
        let Some(Native::PacketField(owner, record, offset)) = self.source(*address) else {
            return Err("event address has no proven managed owner");
        };
        let event = self
            .events
            .iter()
            .find(|event| event.owner == record && i64::from(event.source_offset) == offset)
            .copied()
            .ok_or("event address does not match a declared event slot")?;
        let name = operation.name(event);
        match (operation, rest) {
            (Operation::Length, []) => self.replaceCall(inst, &name, &[owner], I64),
            (Operation::Handler, [index]) if self.source(*index).is_none() => {
                let call = self.callBefore(inst, &name, &[owner, *index], Some(I32));
                self.eraseResult(inst, Native::Packet(self.function.dfg.first_result(call), event.handler))
            }
            (Operation::Subscribe, [handler, capacity])
                if self.constant(*capacity) == Some(i64::from(event.capacity)) =>
            {
                let handler = self.event_handler(*handler, event.handler)?;
                self.replaceCall(inst, &name, &[owner, handler], I64)
            }
            (Operation::Unsubscribe, [handler]) => {
                let handler = self.event_handler(*handler, event.handler)?;
                self.replaceCall(inst, &name, &[owner, handler], I64)
            }
            _ => Err("event service disagrees with its checked call shape"),
        }
    }

    fn event_handler(&self, value: Value, layout: usize) -> Result<Value, &'static str> {
        self.source(value)
            .and_then(|source| ReferenceKind::Record(layout).handle(source))
            .ok_or("event handler does not have the compiled delegate layout")
    }
}
