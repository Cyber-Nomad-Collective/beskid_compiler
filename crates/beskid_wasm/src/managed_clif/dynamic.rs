//! Translate the sealed Dynamic provider to typed Wasm GC allocations and field operations.

mod mapping;

use super::*;
use beskid_codegen::{BrowserDynamicBridge, BrowserDynamicOperation as Operation, LoweredFunction};
use beskid_queries::SourceNominalType;
use cranelift_codegen::ir::{BlockArg, Type, condcodes::IntCC};
use std::collections::BTreeSet;

pub(crate) const ISSUE_TOKEN: &str = "__beskid_dynamic_token_issue";
pub(crate) const RESOLVE_TOKEN: &str = "__beskid_dynamic_token_resolve";

pub(crate) fn admits_token_service(name: &str, signature: &Signature) -> bool {
    matches!(name, ISSUE_TOKEN | RESOLVE_TOKEN)
        && signature.params == [AbiParam::new(I64)]
        && signature.returns == [AbiParam::new(I64)]
}

#[derive(Clone, Copy)]
struct Shape {
    ordinal: i64,
    boxed: usize,
    field: Field,
    unit: bool,
}

pub(super) struct DynamicProgram {
    replacements: HashMap<String, Function>,
    extra: Vec<LoweredFunction>,
}

impl DynamicProgram {
    pub(super) fn prepare(
        artifact: &CodegenArtifact,
        nominals: &HashMap<&SourceNominalType, usize>,
        records: &mut [Vec<Field>],
        arrays: &mut BTreeSet<ArrayElement>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut result = Self { replacements: HashMap::new(), extra: Vec::new() };
        let bindings = &artifact.browser_dynamic;
        if bindings.is_empty() {
            return Ok(result);
        }
        let initialization =
            artifact.dynamic_initialization.as_ref().ok_or("Dynamic bindings have no sealed registration")?;
        let record = |ty: &SourceSignatureType| match ty {
            SourceSignatureType::Nominal(nominal) => {
                nominals.get(nominal).copied().ok_or("Dynamic source layout missing")
            }
            _ => Err("Dynamic signature lacks exact nominal source type"),
        };
        let factory_parameter = |factory| -> Result<Option<usize>, Box<dyn Error>> {
            let Some(symbol) = bindings.factory(factory) else {
                return Ok(None);
            };
            let source =
                artifact.source_signatures.get(symbol).ok_or("Dynamic Result factory missing source signature")?;
            let [parameter] = source.parameters.as_ref() else {
                return Err("Dynamic Result factory has wrong arity".into());
            };
            Ok(Some(record(parameter)?))
        };
        let mut payload = factory_parameter("PackedResultV1")?;
        if payload.is_none() {
            payload = bindings
                .bridges()
                .iter()
                .find(|(_, bridge)| bridge.unpack)
                .and_then(|(symbol, _)| artifact.source_signatures.get(symbol))
                .and_then(|signature| signature.parameters.first())
                .map(record)
                .transpose()?;
        }
        let mut cell = factory_parameter("BrowserDynamicValueResultV1")?;
        if cell.is_none() {
            cell = bindings
                .operations()
                .iter()
                .find(|(_, op)| **op == Operation::Cast)
                .and_then(|(symbol, _)| artifact.source_signatures.get(symbol))
                .and_then(|signature| signature.parameters.first())
                .map(record)
                .transpose()?;
        }
        if let Some(payload) = payload {
            if !records[payload].is_empty() {
                return Err("canonical Dynamic payload is no longer sealed".into());
            }
            records[payload] = vec![Field::Scalar(Scalar::I64), Field::AnyRecord];
        }
        if let Some(cell) = cell {
            if !records[cell].is_empty() || Some(cell) == payload {
                return Err("canonical Dynamic cell is no longer sealed".into());
            }
            records[cell] = vec![Field::Record(payload.ok_or("Dynamic cell lacks canonical payload")?)];
        }

        let mut shapes = Vec::new();
        let mut shape_getters = HashMap::new();
        let mut requests = HashMap::new();
        for (index, issued) in initialization.signatures().iter().enumerate() {
            let mut plans = artifact
                .aggregate_static_plans
                .iter()
                .filter(|plan| plan.descriptor_getter.as_deref() == Some(issued.payload_getter()));
            let plan = plans.next().ok_or("Dynamic payload descriptor was not retained")?;
            if plans.next().is_some() {
                return Err("Dynamic payload descriptor is ambiguous".into());
            }
            let nominal = artifact
                .aggregate_declarations
                .get(&plan.allocation_request_symbol)
                .ok_or("Dynamic Pack box source identity missing")?;
            let boxed = *nominals.get(nominal).ok_or("Dynamic Pack box GC layout missing")?;
            let [field] = records[boxed].as_slice() else {
                return Err("canonical Dynamic Pack box must have one field".into());
            };
            let ordinal = i64::try_from(index)? + 1;
            let shape = Shape { ordinal, boxed, field: *field, unit: plan.fields[0].abi_type == SemanticTypeId::UNIT };
            shapes.push(shape);
            requests.insert(plan.allocation_request_symbol.as_str(), shape);
            shape_getters.insert(issued.shape_tag_getter(), ordinal);
        }
        let mut mapping_getters = HashMap::new();
        let mut transforms = Vec::new();
        for (index, plan) in initialization.mappings().iter().enumerate() {
            let ordinal = i64::try_from(shapes.len() + index + 1)?;
            mapping_getters.insert(plan.token_getter(), ordinal);
            transforms.push(mapping::Transform {
                ordinal,
                source: shapes[plan.source_index()],
                destination: shapes[plan.destination_index()],
                plan: plan.plan(),
            });
        }

        for lowered in &artifact.functions {
            let name = &lowered.name;
            let signature = &lowered.function.signature;
            let replacement = if let Some(bridge) = bindings.bridges().get(name) {
                let shape = *requests.get(bridge.request.as_str()).ok_or("Dynamic bridge shape was not registered")?;
                emit_bridge(signature, payload.ok_or("Dynamic bridge has no canonical payload type")?, shape, bridge)?
            } else if let Some(&operation) = bindings.operations().get(name) {
                let [ok, err] = operation.factory_names().map(|factory| bindings.factory(factory));
                let ok = ok.ok_or("Dynamic success factory was not retained")?;
                let err = err.ok_or("Dynamic error factory was not retained")?;
                emit_operation(
                    signature,
                    operation,
                    payload.ok_or("Dynamic payload type missing")?,
                    cell.ok_or("Dynamic cell type missing")?,
                    ok,
                    err,
                    &transforms,
                )?
            } else if bindings.getters().contains(name) {
                let names = lowered
                    .function
                    .dfg
                    .ext_funcs
                    .values()
                    .filter_map(|external| match &external.name {
                        ExternalName::TestCase(symbol) => std::str::from_utf8(symbol.raw()).ok(),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [getter] = names.as_slice() else {
                    if names.is_empty() {
                        continue;
                    } // A rejected mapping's existing zero result.
                    return Err("Dynamic registration getter has ambiguous provider references".into());
                };
                let ordinal = shape_getters
                    .get(*getter)
                    .or_else(|| mapping_getters.get(*getter))
                    .ok_or("Dynamic getter was not registered")?;
                emit_getter(signature, *ordinal)?
            } else {
                continue;
            };
            if !initialization.admits_function(lowered) {
                return Err(format!("Dynamic provider source seal mismatch for {name}").into());
            }
            verify_function(&replacement, &settings::Flags::new(settings::builder()))?;
            result.replacements.insert(name.clone(), replacement);
        }
        if artifact.functions.iter().any(|lowered| bindings.operations().get(&lowered.name) == Some(&Operation::Map)) {
            let call_conv =
                artifact.functions.first().ok_or("Dynamic artifact has no functions")?.function.signature.call_conv;
            for transform in transforms {
                transform.emit(call_conv, nominals, records, arrays, &mut result.extra)?;
            }
        }
        Ok(result)
    }

    pub(super) fn replace(&mut self, lowered: &mut LoweredFunction) -> bool {
        if let Some(function) = self.replacements.remove(&lowered.name) {
            lowered.function = function;
            true
        } else {
            false
        }
    }

    pub(super) fn finish(self, functions: &mut Vec<LoweredFunction>) {
        functions.extend(self.extra);
    }
}

pub(super) fn requires_box(artifact: &CodegenArtifact, plan: &beskid_codegen::AggregateStaticPlan) -> bool {
    if artifact.browser_dynamic.is_empty() {
        return false;
    }
    let Some(getter) = plan.descriptor_getter.as_deref() else {
        return false;
    };
    artifact.dynamic_initialization.as_ref().is_some_and(|initialization| {
        initialization.signatures().iter().any(|issued| issued.payload_getter() == getter)
    })
}

fn field_type(field: Field) -> Type {
    match field {
        Field::Scalar(kind) => scalar_type(kind),
        _ => I32,
    }
}

fn reference_field(field: Field) -> bool {
    !matches!(field, Field::Scalar(_))
}

struct Builder {
    function: Function,
    block: Block,
}

impl Builder {
    fn new(signature: &Signature) -> (Self, Vec<Value>) {
        let mut function = Function::new();
        function.signature = signature.clone();
        let block = function.dfg.make_block();
        function.layout.append_block(block);
        let params =
            signature.params.iter().map(|param| function.dfg.append_block_param(block, param.value_type)).collect();
        (Self { function, block }, params)
    }

    fn block(&mut self) -> Block {
        let block = self.function.dfg.make_block();
        self.function.layout.append_block(block);
        block
    }

    fn cursor(&mut self) -> FuncCursor<'_> {
        let mut cursor = FuncCursor::new(&mut self.function);
        cursor.goto_bottom(self.block);
        cursor
    }

    fn call(&mut self, symbol: &str, args: &[Value], result: Option<Type>) -> Option<Value> {
        let mut signature = Signature::new(self.function.signature.call_conv);
        signature.params.extend(args.iter().map(|&value| AbiParam::new(self.function.dfg.value_type(value))));
        signature.returns.extend(result.map(AbiParam::new));
        let signature = self.function.import_signature(signature);
        let callee = self.function.import_function(ExtFuncData {
            name: ExternalName::testcase(symbol),
            signature,
            colocated: false,
            patchable: false,
        });
        let call = self.cursor().ins().call(callee, args);
        result.map(|_| self.function.dfg.first_result(call))
    }

    fn alloc(&mut self, record: usize) -> Value {
        self.call(&RecordOp::Alloc.symbol(record), &[], Some(I32)).unwrap()
    }

    fn get(&mut self, record: usize, field: usize, handle: Value, ty: Type) -> Value {
        self.call(&RecordOp::Get(field).symbol(record), &[handle], Some(ty)).unwrap()
    }

    fn set(&mut self, record: usize, field: usize, handle: Value, value: Value) {
        self.call(&RecordOp::Set(field).symbol(record), &[handle, value], None);
    }

    fn payload(&mut self, record: usize, shape: Shape, boxed: Value) -> Value {
        let handle = self.alloc(record);
        let ordinal = self.cursor().ins().iconst(I64, shape.ordinal);
        self.set(record, 0, handle, ordinal);
        self.set(record, 1, handle, boxed);
        handle
    }

    fn cell(&mut self, record: usize, payload: Value) -> Value {
        let value = self.alloc(record);
        self.set(record, 0, value, payload);
        value
    }

    fn branch(&mut self, condition: Value, yes: Block, no: Block) {
        self.cursor().ins().brif(condition, yes, &[], no, &[]);
    }

    fn return_factory(&mut self, factory: &str, args: &[Value]) {
        let result = self.call(factory, args, Some(I64)).unwrap();
        self.cursor().ins().return_(&[result]);
    }

    fn return_reference(&mut self, factory: &str, value: Value) {
        let argument = self.cursor().ins().uextend(I64, value);
        self.return_factory(factory, &[argument]);
    }
}

fn callable(signature: &Signature, params: &[Type]) -> Result<(), &'static str> {
    if signature.params.iter().map(|param| param.value_type).eq(params.iter().copied())
        && signature.returns == [AbiParam::new(I64)]
    {
        Ok(())
    } else {
        Err("canonical Dynamic bridge has wrong CLIF signature")
    }
}

fn emit_getter(signature: &Signature, ordinal: i64) -> Result<Function, Box<dyn Error>> {
    callable(signature, &[])?;
    let (mut b, _) = Builder::new(signature);
    let ordinal = b.cursor().ins().iconst(I64, ordinal);
    let token = b.call(ISSUE_TOKEN, &[ordinal], Some(I64)).unwrap();
    b.cursor().ins().return_(&[token]);
    Ok(b.function)
}

fn emit_bridge(
    signature: &Signature,
    payload_record: usize,
    shape: Shape,
    bridge: &BrowserDynamicBridge,
) -> Result<Function, Box<dyn Error>> {
    callable(signature, &[I64])?;
    let (mut b, params) = Builder::new(signature);
    let supplied = b.cursor().ins().ireduce(I32, params[0]);
    let valid = b.block();
    let invalid = b.block();
    if bridge.unpack {
        let check = b.block();
        b.branch(supplied, check, invalid);
        b.block = check;
        let ordinal = b.get(payload_record, 0, supplied, I64);
        let matches = b.cursor().ins().icmp_imm_u(IntCC::Equal, ordinal, shape.ordinal);
        b.branch(matches, valid, invalid);
        b.block = valid;
        if shape.unit {
            b.return_factory(&bridge.valid, &[]);
        } else {
            let boxed = b.get(payload_record, 1, supplied, I32);
            let argument = b.get(shape.boxed, 0, boxed, field_type(shape.field));
            if reference_field(shape.field) {
                b.return_reference(&bridge.valid, argument);
            } else {
                b.return_factory(&bridge.valid, &[argument]);
            }
        }
    } else {
        b.branch(supplied, valid, invalid);
        b.block = valid;
        let payload = b.payload(payload_record, shape, supplied);
        b.return_reference(&bridge.valid, payload);
    }
    b.block = invalid;
    b.return_factory(&bridge.invalid, &[]);
    Ok(b.function)
}

fn emit_operation(
    signature: &Signature,
    operation: Operation,
    payload_record: usize,
    cell_record: usize,
    success: &str,
    error: &str,
    transforms: &[mapping::Transform<'_>],
) -> Result<Function, Box<dyn Error>> {
    callable(signature, &[I64, I64])?;
    let (mut b, params) = Builder::new(signature);
    let supplied = b.cursor().ins().ireduce(I32, params[0]);
    let token = params[1];
    let ready = b.block();
    let invalid = b.block();
    let foreign = b.block();
    let token_present = b.cursor().ins().icmp_imm_u(IntCC::NotEqual, token, 0);
    let supplied_present = b.cursor().ins().icmp_imm_u(IntCC::NotEqual, supplied, 0);
    let present = b.cursor().ins().band(token_present, supplied_present);
    b.branch(present, ready, invalid);
    b.block = ready;

    let payload = if operation == Operation::Create { supplied } else { b.get(cell_record, 0, supplied, I32) };
    let ordinal = b.get(payload_record, 0, payload, I64);
    let token_ordinal = b.call(RESOLVE_TOKEN, &[token], Some(I64)).unwrap();
    if operation == Operation::Map {
        for transform in transforms {
            let matched = b.block();
            let next = b.block();
            let same_token = b.cursor().ins().icmp_imm_u(IntCC::Equal, token_ordinal, transform.ordinal);
            let same_source = b.cursor().ins().icmp_imm_u(IntCC::Equal, ordinal, transform.source.ordinal);
            let allowed = b.cursor().ins().band(same_token, same_source);
            b.branch(allowed, matched, next);
            b.block = matched;
            let boxed = b.get(payload_record, 1, payload, I32);
            let mapped = b.call(&transform.symbol(), &[boxed], Some(I32)).unwrap();
            let mapped_ok = b.block();
            b.branch(mapped, mapped_ok, foreign);
            b.block = mapped_ok;
            let payload = b.payload(payload_record, transform.destination, mapped);
            let cell = b.cell(cell_record, payload);
            b.return_reference(success, cell);
            b.block = next;
        }
        b.cursor().ins().jump(foreign, &[]);
    } else {
        let valid = b.block();
        let matches = b.cursor().ins().icmp(IntCC::Equal, token_ordinal, ordinal);
        b.branch(matches, valid, foreign);
        b.block = valid;
        let value = if operation == Operation::Create { b.cell(cell_record, payload) } else { payload };
        b.return_reference(success, value);
    }
    for (block, status) in [(invalid, 1), (foreign, 2)] {
        b.block = block;
        let status = b.cursor().ins().iconst(I64, status);
        b.return_factory(error, &[status]);
    }
    Ok(b.function)
}
