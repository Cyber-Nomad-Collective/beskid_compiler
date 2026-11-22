//! Compile the shared, approved mapping graph using the browser's typed GC operations.

use super::*;
use beskid_codegen::dynamic_mapping::MAX_RUNTIME_DEPTH;
use beskid_queries::{DynamicMappingPlan, DynamicMappingStep, DynamicPackingNode, DynamicPackingShape};
use cranelift_codegen::isa::CallConv;

pub(super) struct Transform<'a> {
    pub ordinal: i64,
    pub source: Shape,
    pub destination: Shape,
    pub plan: &'a DynamicMappingPlan,
}

impl Transform<'_> {
    pub fn symbol(&self) -> String {
        format!("__beskid_browser_dynamic_map_{}", self.ordinal)
    }

    fn step_symbol(&self, step: u32) -> String {
        format!("{}_step_{step}", self.symbol())
    }

    pub fn emit(
        self,
        call_conv: CallConv,
        nominal_layouts: &HashMap<&SourceNominalType, usize>,
        records: &[Vec<Field>],
        arrays: &mut BTreeSet<ArrayElement>,
        output: &mut Vec<LoweredFunction>,
    ) -> Result<(), Box<dyn Error>> {
        let source = Graph { shape: self.plan.source(), nominal_layouts };
        let destination = Graph { shape: self.plan.destination(), nominal_layouts };
        let mut signature = Signature::new(call_conv);
        signature.params.push(AbiParam::new(I32));
        signature.returns.push(AbiParam::new(I32));
        let (mut builder, args) = Builder::new(&signature);
        let failed = builder.block();
        let input = builder.get(self.source.boxed, 0, args[0], field_type(self.source.field));
        let depth = builder.cursor().ins().iconst(I32, 0);
        let value =
            self.transfer(0, Some((input, self.source.field)), self.destination.field, &mut builder, depth, failed)?;
        let boxed = builder.alloc(self.destination.boxed);
        builder.set(self.destination.boxed, 0, boxed, value);
        builder.cursor().ins().return_(&[boxed]);
        null_return(&mut builder, failed);
        add(output, self.symbol(), builder.function)?;

        signature.params.push(AbiParam::new(I32));
        for (index, step) in self.plan.steps().iter().enumerate() {
            let (mut builder, args) = Builder::new(&signature);
            let left = args[0];
            let depth = args[1];
            let failed = builder.block();
            match step {
                DynamicMappingStep::Record { source: from, destination: to, fields } => {
                    let source = source.record(*from)?;
                    let destination = destination.record(*to)?;
                    check_input(&mut builder, left, depth, failed);
                    let result = builder.alloc(destination);
                    for field in fields {
                        let ty =
                            *records[destination].get(field.destination).ok_or("Dynamic destination field missing")?;
                        let input = field
                            .source
                            .map(|index| {
                                let ty = *records[source].get(index).ok_or("Dynamic source field missing")?;
                                Ok::<_, &'static str>((builder.get(source, index, left, field_type(ty)), ty))
                            })
                            .transpose()?;
                        let value = self.transfer(field.step, input, ty, &mut builder, depth, failed)?;
                        builder.set(destination, field.destination, result, value);
                    }
                    builder.cursor().ins().return_(&[result]);
                }
                DynamicMappingStep::Array { source: from, destination: to, element } => {
                    let from = source.array(*from)?;
                    let to = destination.array(*to)?;
                    arrays.extend([from, to]);
                    check_input(&mut builder, left, depth, failed);
                    let length = builder.call(&ArrayOp::Len.symbol(from), &[left], Some(I32)).unwrap();
                    let result = builder.call(&ArrayOp::Alloc.symbol(to), &[length], Some(I32)).unwrap();
                    let loop_block = builder.block();
                    let index = builder.function.dfg.append_block_param(loop_block, I32);
                    let body = builder.block();
                    let done = builder.block();
                    let zero = builder.cursor().ins().iconst(I32, 0);
                    builder.cursor().ins().jump(loop_block, &[BlockArg::Value(zero)]);
                    builder.block = loop_block;
                    let more = builder.cursor().ins().icmp(IntCC::UnsignedLessThan, index, length);
                    builder.branch(more, body, done);
                    builder.block = body;
                    let input_type = element_field(from);
                    let input =
                        builder.call(&ArrayOp::Get.symbol(from), &[left, index], Some(field_type(input_type))).unwrap();
                    let value = self.transfer(
                        *element,
                        Some((input, input_type)),
                        element_field(to),
                        &mut builder,
                        depth,
                        failed,
                    )?;
                    builder.call(&ArrayOp::Set.symbol(to), &[result, index, value], None);
                    let next = builder.cursor().ins().iadd_imm_s(index, 1);
                    builder.cursor().ins().jump(loop_block, &[BlockArg::Value(next)]);
                    builder.block = done;
                    builder.cursor().ins().return_(&[result]);
                }
                DynamicMappingStep::OptionalNone { destination: node, ordinal } => {
                    let record = destination.record(*node)?;
                    if records[record].first() != Some(&Field::Scalar(Scalar::I32)) {
                        return Err("Dynamic optional target has no enum tag".into());
                    }
                    let none = builder.alloc(record);
                    let discriminant = builder.cursor().ins().iconst(I32, i64::from(*ordinal));
                    builder.set(record, 0, none, discriminant);
                    builder.cursor().ins().return_(&[none]);
                }
                _ => continue,
            }
            null_return(&mut builder, failed);
            add(output, self.step_symbol(u32::try_from(index)?), builder.function)?;
        }
        Ok(())
    }

    fn transfer(
        &self,
        step: u32,
        input: Option<(Value, Field)>,
        output: Field,
        builder: &mut Builder,
        depth: Value,
        failed: Block,
    ) -> Result<Value, Box<dyn Error>> {
        let required = || input.ok_or("Dynamic mapping input field missing");
        let output_type = field_type(output);
        let value = match self.plan.steps().get(step as usize).ok_or("Dynamic mapping step outside plan")? {
            DynamicMappingStep::Exact { .. } | DynamicMappingStep::Shared { .. } => {
                let (value, input_type) = required()?;
                if input_type != output {
                    return Err("Dynamic exact copy disagrees with source field types".into());
                }
                value
            }
            DynamicMappingStep::Widen { source, .. } => {
                let (value, input_type) = required()?;
                if reference_field(input_type)
                    || reference_field(output)
                    || field_type(input_type).bits() >= output_type.bits()
                {
                    return Err("Dynamic widened field has invalid scalar ABI".into());
                }
                if source.starts_with('i') {
                    builder.cursor().ins().sextend(output_type, value)
                } else {
                    builder.cursor().ins().uextend(output_type, value)
                }
            }
            DynamicMappingStep::Record { .. }
            | DynamicMappingStep::Array { .. }
            | DynamicMappingStep::OptionalNone { .. } => {
                if !reference_field(output) {
                    return Err("Dynamic composite result is not a managed reference".into());
                }
                let value = match input {
                    Some((value, from)) if reference_field(from) => value,
                    None => builder.cursor().ins().iconst(I32, 0),
                    _ => return Err("Dynamic composite input is not a managed reference".into()),
                };
                let next_depth = builder.cursor().ins().iadd_imm_s(depth, 1);
                builder.call(&self.step_symbol(step), &[value, next_depth], Some(I32)).unwrap()
            }
        };
        if reference_field(output) {
            let retain = builder.block();
            builder.branch(value, retain, failed);
            builder.block = retain;
        }
        Ok(value)
    }
}

fn add(output: &mut Vec<LoweredFunction>, name: String, function: Function) -> Result<(), Box<dyn Error>> {
    verify_function(&function, &settings::Flags::new(settings::builder()))?;
    output.push(LoweredFunction { name, function });
    Ok(())
}

fn null_return(builder: &mut Builder, failed: Block) {
    builder.block = failed;
    let zero = builder.cursor().ins().iconst(I32, 0);
    builder.cursor().ins().return_(&[zero]);
}

fn check_input(builder: &mut Builder, handle: Value, depth: Value, failed: Block) {
    let next = builder.block();
    let nonnull = builder.cursor().ins().icmp_imm_u(IntCC::NotEqual, handle, 0);
    let finite = builder.cursor().ins().icmp_imm_u(IntCC::UnsignedLessThanOrEqual, depth, i64::from(MAX_RUNTIME_DEPTH));
    let valid = builder.cursor().ins().band(nonnull, finite);
    builder.branch(valid, next, failed);
    builder.block = next;
}

fn element_field(element: ArrayElement) -> Field {
    match element {
        ArrayElement::Scalar(scalar) => Field::Scalar(scalar),
        ArrayElement::Record(record) => Field::Record(record),
        ArrayElement::String => Field::String,
        nested => Field::Array(nested.inner().expect("nested array")),
    }
}

struct Graph<'a> {
    shape: &'a DynamicPackingShape,
    nominal_layouts: &'a HashMap<&'a SourceNominalType, usize>,
}

impl Graph<'_> {
    fn source_type(&self, index: u32, depth: usize) -> Result<SourceSignatureType, Box<dyn Error>> {
        if depth > 64 {
            return Err("Dynamic type argument depth exceeds the verified shape bound".into());
        }
        let node = self.shape.nodes().get(index as usize).ok_or("Dynamic shape node outside graph")?;
        match node {
            DynamicPackingNode::Scalar { name, .. } => {
                use SemanticTypeId as S;
                let scalar = match *name {
                    "bool" => S::BOOL,
                    "i8" => S::I8,
                    "i16" => S::I16,
                    "i32" => S::I32,
                    "i64" => S::I64,
                    "u8" => S::U8,
                    "u16" => S::U16,
                    "u32" => S::U32,
                    "u64" => S::U64,
                    "word" => S::WORD,
                    "f32" => S::F32,
                    "f64" => S::F64,
                    "char" => S::CHAR,
                    "utf8" => S::STRING,
                    "unit" => S::UNIT,
                    _ => return Err("unsupported Dynamic scalar shape".into()),
                };
                Ok(SourceSignatureType::Scalar(scalar))
            }
            DynamicPackingNode::Array { element } => {
                Ok(SourceSignatureType::Array(Box::new(self.source_type(*element, depth + 1)?)))
            }
            DynamicPackingNode::Record { declaration, arguments, .. }
            | DynamicPackingNode::Enum { declaration, arguments, .. } => {
                let arguments =
                    arguments.iter().map(|&arg| self.source_type(arg, depth + 1)).collect::<Result<_, _>>()?;
                Ok(SourceSignatureType::Nominal(SourceNominalType { declaration: *declaration, arguments }))
            }
        }
    }

    fn record(&self, index: u32) -> Result<usize, Box<dyn Error>> {
        let SourceSignatureType::Nominal(nominal) = self.source_type(index, 0)? else {
            return Err("Dynamic mapping expected a nominal source node".into());
        };
        self.nominal_layouts
            .get(&nominal)
            .copied()
            .ok_or_else(|| format!("Dynamic mapping has no GC layout for {nominal:?}").into())
    }

    fn array(&self, index: u32) -> Result<ArrayElement, Box<dyn Error>> {
        let SourceSignatureType::Array(element) = self.source_type(index, 0)? else {
            return Err("Dynamic mapping expected an array node".into());
        };
        self.element(&element)
    }

    fn element(&self, ty: &SourceSignatureType) -> Result<ArrayElement, Box<dyn Error>> {
        match ty {
            SourceSignatureType::Scalar(ty) => {
                array_element(*ty).ok_or_else(|| "unsupported Dynamic array scalar".into())
            }
            SourceSignatureType::Nominal(nominal) => self
                .nominal_layouts
                .get(nominal)
                .copied()
                .map(ArrayElement::Record)
                .ok_or_else(|| "Dynamic array record GC layout missing".into()),
            SourceSignatureType::Array(element) => {
                self.element(element)?.nested().ok_or_else(|| "Dynamic nested array depth overflow".into())
            }
            _ => Err("unsupported Dynamic array element".into()),
        }
    }
}
