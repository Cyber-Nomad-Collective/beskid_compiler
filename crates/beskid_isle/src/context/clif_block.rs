//! Lowering for `clif { ... }` blocks.
//!
//! The surface (statement split, `%` names, opcode allowlist, pseudo-instructions) is owned by
//! [`beskid_analysis::clif_surface`]. Here every run of consecutive instructions is rendered as
//! a synthetic single-block CLIF function whose entry parameters are the run's live-in values,
//! parsed with `cranelift-reader`, verified on its own, and copied instruction by instruction
//! into the enclosing function with values remapped. `payload`, `length`, `call`, and `return`
//! are emitted directly.
//!
//! Memory safety contract: loads and stores are only accepted at addresses derived from a
//! `payload %N` value by `iadd`/`isub` with a non-address offset. Bounds are the caller's
//! responsibility (a precondition of the enclosing Beskid function). A block that reads a payload
//! cannot call, so no safepoint can run while a derived address is live; Beskid's collector is
//! non-moving, so the payload of an array parameter is stable for the block's duration.

use std::collections::{HashMap, HashSet};

use beskid_analysis::clif_surface::{
    CLIF_ALLOWED_OPCODES, CLIF_LOAD_OPCODES, CLIF_STORE_OPCODES, ClifBlockSurface, ClifOperand, ClifStatement,
    parse_clif_surface,
};
use beskid_queries::ClifParameterShape;
use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, Function, InstBuilder, InstBuilderBase, InstructionData, MemFlagsData, Opcode,
    Signature,
    TrapCode, Type, Value, types,
};
use cranelift_codegen::settings;

use super::IsleContext;
use crate::errors::{LoweringError, LoweringErrorKind};
use crate::facts::AstNodeKey;

/// Lower one CLIF block, recording an [`LoweringErrorKind::InvalidClifBlock`] on failure.
pub(super) fn lower_clif_block(context: &mut IsleContext<'_, '_, '_, '_>, key: AstNodeKey) -> Option<Value> {
    let result = (|| {
        let body = context.facts.clif_block_body(key).ok_or_else(|| "clif block body is unavailable".to_owned())?;
        let surface = parse_clif_surface(&body).map_err(|error| error.to_string())?;
        let result_type = context.facts.scalar_type(key).or_else(|| {
            context.builder.func.signature.returns.first().map(|parameter| parameter.value_type)
        });
        let result_type = result_type.ok_or_else(|| "clif block has no typed context".to_owned())?;
        let shapes = context.facts.clif_block_parameters(key);
        ClifLowering {
            context: &mut *context,
            shapes,
            result_type,
            locals: HashMap::new(),
            addresses: HashSet::new(),
            remapped: HashMap::new(),
        }
            .lower(&surface)
    })();
    match result {
        Ok(value) => Some(value),
        Err(message) => {
            context.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidClifBlock(message) });
            None
        }
    }
}

struct ClifLowering<'c, 'b, 'f, 'facts, 'i> {
    context: &'c mut IsleContext<'b, 'f, 'facts, 'i>,
    /// `None` only for fact providers without parameter shapes (unit-test fixtures); production
    /// codegen always supplies shapes, so payload access then fails closed.
    shapes: Option<Vec<ClifParameterShape>>,
    result_type: Type,
    locals: HashMap<String, Value>,
    /// Values derived from a `payload` base address.
    addresses: HashSet<Value>,
    /// Synthetic-function values mapped to their copies in the enclosing function.
    remapped: HashMap<Value, Value>,
}

impl ClifLowering<'_, '_, '_, '_, '_> {
    fn lower(mut self, surface: &ClifBlockSurface) -> Result<Value, String> {
        let mut pending: Vec<&ClifStatement> = Vec::new();
        for statement in &surface.statements {
            if let ClifStatement::Instruction { .. } = statement {
                pending.push(statement);
                continue;
            }
            if !pending.is_empty() {
                self.emit_instructions(&pending)?;
                pending.clear();
            }
            match statement {
                ClifStatement::Instruction { .. } => unreachable!("instructions are batched above"),
                ClifStatement::Payload { line, result, parameter } => {
                    let value = self.array_field(*line, *parameter, 0)?;
                    self.addresses.insert(value);
                    self.locals.insert(result.clone(), value);
                }
                ClifStatement::Length { line, result, parameter } => {
                    let pointer_bytes = i32::try_from(self.pointer_type().bytes()).map_err(|error| error.to_string())?;
                    let value = self.array_field(*line, *parameter, pointer_bytes)?;
                    self.locals.insert(result.clone(), value);
                }
                ClifStatement::Call { line, result, symbol, arguments, result_type } => {
                    let value = self.emit_call(*line, symbol, arguments, result_type.as_deref())?;
                    match result {
                        Some(name) => {
                            self.locals.insert(name.clone(), value);
                        }
                        None => return self.block_value(*line, value),
                    }
                }
                ClifStatement::Return { line, value } => {
                    let value = self.value_operand(*line, value)?;
                    return self.block_value(*line, value);
                }
            }
        }
        Err("a clif block must end with `return %value`".to_owned())
    }

    fn pointer_type(&self) -> Type {
        self.context.frontend_config.pointer_type()
    }

    fn block_value(&self, line: usize, value: Value) -> Result<Value, String> {
        let actual = self.context.builder.func.dfg.value_type(value);
        if actual != self.result_type {
            return Err(format!(
                "clif block line {line}: the block yields `{actual}` but its context expects `{}`",
                self.result_type
            ));
        }
        Ok(value)
    }

    fn parameter(&self, line: usize, index: usize) -> Result<Value, String> {
        self.context.function_param_values.get(index).copied().ok_or_else(|| {
            format!(
                "clif block line {line}: `%{index}` does not name a parameter; the enclosing function has {} \
                 ABI parameter(s)",
                self.context.function_param_values.len()
            )
        })
    }

    /// Resolve an operand that becomes an ordinary CLIF value (instruction operand, call
    /// argument, or block result). Array and managed parameters are rejected.
    fn operand(&self, line: usize, operand: &ClifOperand) -> Result<Value, String> {
        match operand {
            ClifOperand::Parameter(index) => {
                let value = self.parameter(line, *index)?;
                match self.shapes.as_ref().map(|shapes| shapes.get(*index)) {
                    None | Some(Some(ClifParameterShape::Scalar)) => Ok(value),
                    Some(Some(ClifParameterShape::PayloadArray { .. })) => Err(format!(
                        "clif block line {line}: `%{index}` is an array; read it through `payload %{index}` and \
                         `length %{index}`"
                    )),
                    Some(Some(ClifParameterShape::Opaque)) | Some(None) => {
                        Err(format!("clif block line {line}: `%{index}` is not a scalar parameter"))
                    }
                }
            }
            ClifOperand::Local(name) => self
                .locals
                .get(name)
                .copied()
                .ok_or_else(|| format!("clif block line {line}: `%{name}` is used before it is defined")),
        }
    }

    /// Like [`Self::operand`], but the value leaves the block's address discipline: payload
    /// addresses may not be returned or passed to calls.
    fn value_operand(&self, line: usize, operand: &ClifOperand) -> Result<Value, String> {
        let value = self.operand(line, operand)?;
        if self.addresses.contains(&value) {
            return Err(format!(
                "clif block line {line}: payload address `{operand}` cannot leave the block; load or store through it"
            ));
        }
        Ok(value)
    }

    fn array_field(&mut self, line: usize, index: usize, offset: i32) -> Result<Value, String> {
        let shape = self.shapes.as_ref().and_then(|shapes| shapes.get(index).copied());
        let Some(ClifParameterShape::PayloadArray { .. }) = shape else {
            return Err(format!(
                "clif block line {line}: `payload`/`length` require a `u8[]`, `u32[]`, or `i64[]` parameter, and \
                 `%{index}` is not one"
            ));
        };
        let array = self.parameter(line, index)?;
        let pointer_type = self.pointer_type();
        if self.context.builder.func.dfg.value_type(array) != pointer_type {
            return Err(format!("clif block line {line}: `%{index}` is not a managed array reference"));
        }
        let builder = &mut self.context.builder;
        builder.ins().trapz(array, TrapCode::unwrap_user(1));
        Ok(builder.ins().load(pointer_type, MemFlagsData::trusted(), array, offset))
    }

    fn emit_call(
        &mut self,
        line: usize,
        symbol: &str,
        arguments: &[ClifOperand],
        annotation: Option<&str>,
    ) -> Result<Value, String> {
        let arguments =
            arguments.iter().map(|argument| self.value_operand(line, argument)).collect::<Result<Vec<_>, _>>()?;
        let return_type = match annotation {
            Some(name) => scalar_type_named(name)
                .ok_or_else(|| format!("clif block line {line}: unsupported call result type `{name}`"))?,
            None => self.result_type,
        };
        let builder = &mut self.context.builder;
        let mut signature = Signature::new(builder.func.signature.call_conv);
        for argument in &arguments {
            signature.params.push(AbiParam::new(builder.func.dfg.value_type(*argument)));
        }
        signature.returns.push(AbiParam::new(return_type));
        let signature = builder.func.import_signature(signature);
        let callee = builder.func.import_function(ExtFuncData {
            name: ExternalName::testcase(symbol),
            signature,
            colocated: false,
            patchable: false,
        });
        let call = builder.ins().call(callee, &arguments);
        builder
            .inst_results(call)
            .first()
            .copied()
            .ok_or_else(|| format!("clif block line {line}: call to `@{symbol}` produced no value"))
    }

    fn emit_instructions(&mut self, statements: &[&ClifStatement]) -> Result<(), String> {
        // Number every `%` name: live-ins first (entry block parameters `v0..vK`), then the
        // values this run defines, in definition order.
        let mut numbering: HashMap<ClifOperand, u32> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut live_in: Vec<Value> = Vec::new();
        let mut defined: Vec<String> = Vec::new();
        let mut defined_here: HashSet<String> = HashSet::new();
        for statement in statements {
            let ClifStatement::Instruction { line, results, operands, .. } = statement else {
                continue;
            };
            for operand in operands {
                if numbering.contains_key(operand)
                    || matches!(operand, ClifOperand::Local(name) if defined_here.contains(name))
                {
                    continue;
                }
                let value = self.operand(*line, operand)?;
                numbering.insert(operand.clone(), names.len() as u32);
                names.push(operand.to_string());
                live_in.push(value);
            }
            for result in results {
                if !defined_here.insert(result.clone()) || self.locals.contains_key(result) {
                    return Err(format!("clif block line {line}: `%{result}` is defined more than once"));
                }
                defined.push(result.clone());
            }
        }
        for result in &defined {
            numbering.insert(ClifOperand::Local(result.clone()), names.len() as u32);
            names.push(format!("%{result}"));
        }

        let dfg = &self.context.builder.func.dfg;
        let parameter_types = live_in.iter().map(|value| dfg.value_type(*value).to_string()).collect::<Vec<_>>();
        let mut text = format!("function %clif_block({}) system_v {{\nblock0(", parameter_types.join(", "));
        text.push_str(
            &parameter_types
                .iter()
                .enumerate()
                .map(|(index, ty)| format!("v{index}: {ty}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        text.push_str("):\n");
        let mut line_of = Vec::new();
        for statement in statements {
            let ClifStatement::Instruction { line, results, text: instruction, .. } = statement else {
                continue;
            };
            text.push_str("    ");
            if !results.is_empty() {
                let renamed = results
                    .iter()
                    .map(|result| format!("v{}", numbering[&ClifOperand::Local(result.clone())]))
                    .collect::<Vec<_>>();
                text.push_str(&renamed.join(", "));
                text.push_str(" = ");
            }
            text.push_str(&rename_operands(instruction, &numbering));
            text.push('\n');
            line_of.push(*line);
        }
        text.push_str("    return\n}\n");

        let rename_back = |message: &str| restore_names(message, &names);
        let mut functions = cranelift_reader::parse_functions(&text).map_err(|error| {
            // Synthetic line 3 is the first instruction. The reader may only notice a missing
            // operand at the next token, so clamp to the last instruction of this run.
            let line = error
                .location
                .line_number
                .checked_sub(3)
                .and_then(|index| line_of.get(index.min(line_of.len().saturating_sub(1))))
                .map(|line| format!("clif block line {line}: "))
                .unwrap_or_default();
            format!("{line}{}", rename_back(&error.message))
        })?;
        let [_] = functions.as_slice() else {
            return Err("clif block did not parse to one function".to_owned());
        };
        let mut source = functions.remove(0);
        let flags = settings::Flags::new(settings::builder());
        cranelift_codegen::verify_function(&source, &flags)
            .map_err(|errors| format!("clif block does not verify: {}", rename_back(&errors.to_string())))?;

        self.copy_instructions(&mut source, &live_in, &line_of, &names)?;

        for name in defined {
            let number = numbering[&ClifOperand::Local(name.clone())];
            let source_value = source.dfg.resolve_aliases(Value::from_u32(number));
            let value = self
                .remapped
                .get(&source_value)
                .copied()
                .ok_or_else(|| format!("clif block: `%{name}` was not defined by the copied instructions"))?;
            self.locals.insert(name, value);
        }
        self.remapped.clear();
        Ok(())
    }

    fn copy_instructions(
        &mut self,
        source: &mut Function,
        live_in: &[Value],
        line_of: &[usize],
        names: &[String],
    ) -> Result<(), String> {
        let entry = source.layout.entry_block().ok_or_else(|| "clif block has no entry block".to_owned())?;
        let parameters = source.dfg.block_params(entry).to_vec();
        if parameters.len() != live_in.len() {
            return Err("clif block entry parameters do not match its live-in values".to_owned());
        }
        self.remapped.extend(parameters.into_iter().zip(live_in.iter().copied()));
        let instructions = source.layout.block_insts(entry).collect::<Vec<_>>();
        for (position, instruction) in instructions.into_iter().enumerate() {
            let opcode = source.dfg.insts[instruction].opcode();
            if opcode == Opcode::Return {
                break;
            }
            let line = line_of.get(position).copied().unwrap_or_default();
            let opcode_name = opcode.to_string();
            if !CLIF_ALLOWED_OPCODES.contains(&opcode_name.as_str()) {
                return Err(format!("clif block line {line}: opcode `{opcode_name}` is not allowed in a clif block"));
            }
            let arguments = source
                .dfg
                .inst_args(instruction)
                .iter()
                .map(|argument| {
                    let resolved = source.dfg.resolve_aliases(*argument);
                    self.remapped.get(&resolved).copied().ok_or_else(|| {
                        format!(
                            "clif block line {line}: operand `{}` is not defined in this block",
                            restore_names(&resolved.to_string(), names)
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let result_is_address = self.check_address_discipline(line, &opcode_name, &arguments)?;

            let mut data: InstructionData = source.dfg.insts[instruction];
            for (slot, argument) in data.arguments_mut(&mut source.dfg.value_lists).iter_mut().zip(&arguments) {
                *slot = *argument;
            }
            if let Some(flags) = data.memflags_mut() {
                let requested = source.dfg.mem_flags[*flags];
                *flags = self.context.builder.func.dfg.mem_flags.insert_unchecked(payload_mem_flags(requested));
            }
            if let InstructionData::Shuffle { imm, .. } = &mut data {
                let mask = source.dfg.immediates[*imm].clone();
                *imm = self.context.builder.func.dfg.immediates.push(mask);
            }
            let controlling_type = source.dfg.ctrl_typevar(instruction);
            let (copied, dfg) = self.context.builder.ins().build(data, controlling_type);
            let copied_results = dfg.inst_results(copied).to_vec();
            let source_results = source.dfg.inst_results(instruction).to_vec();
            if copied_results.len() != source_results.len() {
                return Err(format!("clif block line {line}: `{opcode_name}` result count changed while copying"));
            }
            for (source_result, copied_result) in source_results.into_iter().zip(copied_results) {
                if result_is_address {
                    self.addresses.insert(copied_result);
                }
                self.remapped.insert(source_result, copied_result);
            }
        }
        Ok(())
    }

    /// Enforce payload address provenance for one instruction. Returns whether its result is
    /// itself a payload-derived address.
    fn check_address_discipline(&self, line: usize, opcode: &str, arguments: &[Value]) -> Result<bool, String> {
        let derived = arguments.iter().map(|argument| self.addresses.contains(argument)).collect::<Vec<_>>();
        if CLIF_LOAD_OPCODES.contains(&opcode) {
            if derived.first() != Some(&true) {
                return Err(format!(
                    "clif block line {line}: `{opcode}` address must derive from `payload %N` plus an offset"
                ));
            }
            return Ok(false);
        }
        if CLIF_STORE_OPCODES.contains(&opcode) {
            if derived.get(1) != Some(&true) {
                return Err(format!(
                    "clif block line {line}: `{opcode}` address must derive from `payload %N` plus an offset"
                ));
            }
            if derived.first() == Some(&true) {
                return Err(format!("clif block line {line}: a payload address cannot be stored to memory"));
            }
            return Ok(false);
        }
        match (opcode, derived.as_slice()) {
            ("iadd", [true, false] | [false, true]) | ("isub", [true, false]) => Ok(true),
            (_, derived) if derived.iter().any(|is_address| *is_address) => Err(format!(
                "clif block line {line}: a payload address may only be offset with `iadd`/`isub` by an integer, or \
                 used as a load/store address (`{opcode}`)"
            )),
            _ => Ok(false),
        }
    }
}

/// Payload accesses keep only the requested endianness and alignment hint. Trap behavior stays
/// the default (an access fault is a trap, never assumed impossible), and alias regions,
/// `readonly`, and `can_move` are dropped because the block cannot prove them.
fn payload_mem_flags(requested: MemFlagsData) -> MemFlagsData {
    let mut flags = MemFlagsData::new();
    if let Some(endianness) = requested.explicit_endianness() {
        flags = flags.with_endianness(endianness);
    }
    if requested.aligned() {
        flags = flags.with_aligned();
    }
    flags
}

/// Replace `%name` operands with the synthetic `vN` names assigned in `numbering`.
fn rename_operands(text: &str, numbering: &HashMap<ClifOperand, u32>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '%' {
            out.push(character);
            continue;
        }
        let mut name = String::new();
        while let Some(&next) = characters.peek() {
            if !(next.is_ascii_alphanumeric() || next == '_') {
                break;
            }
            name.push(next);
            characters.next();
        }
        let operand = if !name.is_empty() && name.chars().all(|character| character.is_ascii_digit()) {
            name.parse::<usize>().ok().map(ClifOperand::Parameter)
        } else {
            Some(ClifOperand::Local(name.clone()))
        };
        match operand.and_then(|operand| numbering.get(&operand)) {
            Some(number) => out.push_str(&format!("v{number}")),
            None => {
                out.push('%');
                out.push_str(&name);
            }
        }
    }
    out
}

/// Map synthetic `vN` names in a reader or verifier message back to the block's `%` names.
fn restore_names(message: &str, names: &[String]) -> String {
    let mut out = String::with_capacity(message.len());
    let bytes = message.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let starts_word = index == 0 || !(bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_');
        if starts_word && bytes[index] == b'v' {
            let digits_end =
                (index + 1..bytes.len()).find(|position| !bytes[*position].is_ascii_digit()).unwrap_or(bytes.len());
            let ends_word = digits_end == bytes.len() || !(bytes[digits_end].is_ascii_alphanumeric() || bytes[digits_end] == b'_');
            if digits_end > index + 1
                && ends_word
                && let Some(name) = message[index + 1..digits_end].parse::<usize>().ok().and_then(|number| names.get(number))
            {
                out.push_str(name);
                index = digits_end;
                continue;
            }
        }
        let character = message[index..].chars().next().expect("index is on a character boundary");
        out.push(character);
        index += character.len_utf8();
    }
    out
}

fn scalar_type_named(name: &str) -> Option<Type> {
    Some(match name {
        "i8" => types::I8,
        "i16" => types::I16,
        "i32" => types::I32,
        "i64" => types::I64,
        "f32" => types::F32,
        "f64" => types::F64,
        _ => return None,
    })
}
