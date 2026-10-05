//! Surface form of `clif { ... }` blocks.
//!
//! A CLIF block is a single straight-line Cranelift basic block written inside Beskid source.
//! This module owns the text-level contract shared by the type checker (which reports
//! diagnostics against declared parameter types) and ISLE lowering (which parses the
//! instruction lines with `cranelift-reader` and copies them into the enclosing function).
//!
//! # Syntax
//!
//! One statement per line. `//` and `;` start a comment that runs to the end of the line.
//!
//! - `%N` (decimal) names parameter `N` of the enclosing function. In a method, `%0` is the
//!   receiver, which is never a usable CLIF value.
//! - `%name` (identifier) names a block-local SSA value; it is defined once and used after its
//!   definition.
//! - `%a, %b = <opcode>[.<type>] <operands>` is a Cranelift instruction from
//!   [`CLIF_ALLOWED_OPCODES`], written in ordinary CLIF text syntax with `%` names in place of
//!   `vN` values.
//! - `%p = payload %N` yields the element base address of array parameter `N`
//!   (`u8[]`, `u32[]`, or `i64[]`); `%n = length %N` yields its element count.
//! - `%r = call @symbol(%a, ...) -> <type>` calls a native symbol; the symbol must be a kit
//!   platform import or a C-ABI `[Extern]` contract method with a library. A result-less
//!   `call @symbol(...)` is allowed only as the final statement and returns the block's type.
//! - `return %x` ends the block and yields `%x` as the block value. A result-less
//!   `call @symbol(...)` as the final statement also yields the call result.
//!
//! Memory access is restricted to addresses derived from a `payload` value by `iadd`/`isub`
//! with an integer offset; the caller is responsible for keeping every access in bounds. A
//! block that reads a payload must not call, so no garbage-collection safepoint can occur
//! while the derived address is live.

use std::collections::HashSet;
use std::fmt;

/// Cranelift opcodes admitted inside a `clif { ... }` block. Everything else, including
/// branches, calls written in raw CLIF syntax, stack slots, globals, and atomics, is rejected.
///
/// Integer division (`udiv`, `sdiv`, `urem`, `srem`) traps on a zero divisor and on signed
/// overflow; callers must exclude those inputs before entering the block.
pub const CLIF_ALLOWED_OPCODES: &[&str] = &[
    // Constants.
    "iconst",
    "f32const",
    "f64const",
    // Integer arithmetic.
    "iadd",
    "isub",
    "ineg",
    "iabs",
    "imul",
    "umulhi",
    "smulhi",
    "udiv",
    "sdiv",
    "urem",
    "srem",
    "smin",
    "umin",
    "smax",
    "umax",
    "uadd_sat",
    "sadd_sat",
    "usub_sat",
    "ssub_sat",
    // Carry, borrow, and overflow arithmetic.
    "uadd_overflow",
    "sadd_overflow",
    "usub_overflow",
    "ssub_overflow",
    "umul_overflow",
    "smul_overflow",
    "uadd_overflow_cin",
    "sadd_overflow_cin",
    "usub_overflow_bin",
    "ssub_overflow_bin",
    // Bitwise operations, shifts, and rotates.
    "band",
    "bor",
    "bxor",
    "bnot",
    "ishl",
    "ushr",
    "sshr",
    "rotl",
    "rotr",
    "clz",
    "cls",
    "ctz",
    "popcnt",
    "bswap",
    "bitrev",
    // Comparison and selection.
    "icmp",
    "select",
    "bitselect",
    "bmask",
    // Conversions.
    "uextend",
    "sextend",
    "ireduce",
    "bitcast",
    // Floating point.
    "fadd",
    "fsub",
    "fmul",
    "fdiv",
    "fneg",
    "fabs",
    "fmin",
    "fmax",
    "sqrt",
    "fcmp",
    "fcvt_from_sint",
    "fcvt_from_uint",
    "fcvt_to_sint_sat",
    "fcvt_to_uint_sat",
    // SIMD lanes.
    "splat",
    "insertlane",
    "extractlane",
    "shuffle",
    "swizzle",
    "vany_true",
    "vall_true",
    "vhigh_bits",
    "snarrow",
    "unarrow",
    "uunarrow",
    "swiden_low",
    "swiden_high",
    "uwiden_low",
    "uwiden_high",
    "iadd_pairwise",
    // Explicit conditional traps.
    "trapz",
    "trapnz",
    // Payload memory access (addresses must derive from `payload`).
    "load",
    "uload8",
    "sload8",
    "uload16",
    "sload16",
    "uload32",
    "sload32",
    "store",
    "istore8",
    "istore16",
    "istore32",
];

/// Loads admitted only with a payload-derived address.
pub const CLIF_LOAD_OPCODES: &[&str] = &["load", "uload8", "sload8", "uload16", "sload16", "uload32", "sload32"];

/// Stores admitted only with a payload-derived address and a non-address value.
pub const CLIF_STORE_OPCODES: &[&str] = &["store", "istore8", "istore16", "istore32"];

/// Array element types whose payload a CLIF block may address, with their element size.
pub const CLIF_PAYLOAD_ELEMENT_TYPES: &[(&str, u8)] = &[("u8", 1), ("u32", 4), ("i64", 8)];

/// One `%` operand.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClifOperand {
    /// `%N`: parameter `N` of the enclosing function.
    Parameter(usize),
    /// `%name`: a block-local value.
    Local(String),
}

impl fmt::Display for ClifOperand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parameter(index) => write!(f, "%{index}"),
            Self::Local(name) => write!(f, "%{name}"),
        }
    }
}

/// One line of a CLIF block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClifStatement {
    /// A Cranelift instruction from [`CLIF_ALLOWED_OPCODES`].
    Instruction {
        line: usize,
        results: Vec<String>,
        opcode: String,
        /// The instruction text after `=` (or the whole line when there are no results), with
        /// `%` operands left in place.
        text: String,
        operands: Vec<ClifOperand>,
    },
    /// `%result = payload %parameter`.
    Payload { line: usize, result: String, parameter: usize },
    /// `%result = length %parameter`.
    Length { line: usize, result: String, parameter: usize },
    /// `[%result =] call @symbol(args) [-> type]`.
    Call { line: usize, result: Option<String>, symbol: String, arguments: Vec<ClifOperand>, result_type: Option<String> },
    /// `return %value`.
    Return { line: usize, value: ClifOperand },
}

impl ClifStatement {
    pub fn line(&self) -> usize {
        match self {
            Self::Instruction { line, .. }
            | Self::Payload { line, .. }
            | Self::Length { line, .. }
            | Self::Call { line, .. }
            | Self::Return { line, .. } => *line,
        }
    }
}

/// A validated CLIF block surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClifBlockSurface {
    pub statements: Vec<ClifStatement>,
}

impl ClifBlockSurface {
    /// Parameters named anywhere in the block, in first-use order.
    pub fn referenced_parameters(&self) -> Vec<usize> {
        let mut seen = Vec::new();
        let mut push = |index: usize| {
            if !seen.contains(&index) {
                seen.push(index);
            }
        };
        for statement in &self.statements {
            match statement {
                ClifStatement::Instruction { operands, .. } | ClifStatement::Call { arguments: operands, .. } => {
                    for operand in operands {
                        if let ClifOperand::Parameter(index) = operand {
                            push(*index);
                        }
                    }
                }
                ClifStatement::Payload { parameter, .. } | ClifStatement::Length { parameter, .. } => push(*parameter),
                ClifStatement::Return { value: ClifOperand::Parameter(index), .. } => push(*index),
                ClifStatement::Return { .. } => {}
            }
        }
        seen
    }

    /// Parameters used as ordinary CLIF values (operands, call arguments, or the return value).
    pub fn value_parameters(&self) -> Vec<usize> {
        let mut seen = Vec::new();
        for statement in &self.statements {
            let operands: &[ClifOperand] = match statement {
                ClifStatement::Instruction { operands, .. } | ClifStatement::Call { arguments: operands, .. } => {
                    operands
                }
                ClifStatement::Return { value, .. } => std::slice::from_ref(value),
                ClifStatement::Payload { .. } | ClifStatement::Length { .. } => &[],
            };
            for operand in operands {
                if let ClifOperand::Parameter(index) = operand
                    && !seen.contains(index)
                {
                    seen.push(*index);
                }
            }
        }
        seen
    }

    /// Parameters addressed through `payload` or `length`.
    pub fn array_parameters(&self) -> Vec<usize> {
        let mut seen = Vec::new();
        for statement in &self.statements {
            if let ClifStatement::Payload { parameter, .. } | ClifStatement::Length { parameter, .. } = statement
                && !seen.contains(parameter)
            {
                seen.push(*parameter);
            }
        }
        seen
    }

    /// Whether the block reads an array payload address.
    pub fn uses_payload(&self) -> bool {
        self.statements.iter().any(|statement| matches!(statement, ClifStatement::Payload { .. }))
    }
}

/// A surface error at a 1-based body line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClifSurfaceError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ClifSurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "clif block line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ClifSurfaceError {}

fn error(line: usize, message: impl Into<String>) -> ClifSurfaceError {
    ClifSurfaceError { line, message: message.into() }
}

/// Parse and validate the surface form of a CLIF block body.
pub fn parse_clif_surface(body: &str) -> Result<ClifBlockSurface, ClifSurfaceError> {
    let mut statements = Vec::new();
    for (index, raw) in body.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        statements.push(parse_statement(line_number, line)?);
    }
    let Some(last) = statements.last() else {
        return Err(error(1, "a clif block must contain at least one statement"));
    };
    let last_line = last.line();
    match last {
        ClifStatement::Return { .. } | ClifStatement::Call { result: None, .. } => {}
        _ => {
            return Err(error(
                last_line,
                "a clif block must end with `return %value` (or a result-less `call @symbol(...)`)",
            ));
        }
    }
    let mut defined = HashSet::new();
    let final_index = statements.len() - 1;
    for (position, statement) in statements.iter().enumerate() {
        let line = statement.line();
        let (uses, defines): (Vec<&ClifOperand>, Vec<&String>) = match statement {
            ClifStatement::Instruction { results, operands, .. } => (operands.iter().collect(), results.iter().collect()),
            ClifStatement::Payload { result, .. } | ClifStatement::Length { result, .. } => {
                (Vec::new(), vec![result])
            }
            ClifStatement::Call { result, arguments, .. } => {
                if result.is_none() && position != final_index {
                    return Err(error(line, "a result-less call is only allowed as the final statement"));
                }
                (arguments.iter().collect(), result.iter().collect())
            }
            ClifStatement::Return { value, .. } => {
                if position != final_index {
                    return Err(error(line, "`return` must be the final statement of a clif block"));
                }
                (vec![value], Vec::new())
            }
        };
        for operand in uses {
            if let ClifOperand::Local(name) = operand
                && !defined.contains(name)
            {
                return Err(error(line, format!("`%{name}` is used before it is defined")));
            }
        }
        for name in defines {
            if !defined.insert(name.clone()) {
                return Err(error(line, format!("`%{name}` is defined more than once")));
            }
        }
    }
    let surface = ClifBlockSurface { statements };
    if surface.uses_payload()
        && let Some(call) = surface.statements.iter().find(|statement| matches!(statement, ClifStatement::Call { .. }))
    {
        return Err(error(
            call.line(),
            "a clif block that reads an array payload must not call: calls are garbage-collection safepoints",
        ));
    }
    Ok(surface)
}

fn strip_comment(line: &str) -> &str {
    let mut end = line.len();
    if let Some(position) = line.find("//") {
        end = end.min(position);
    }
    if let Some(position) = line.find(';') {
        end = end.min(position);
    }
    &line[..end]
}

fn is_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn parse_operand_token(line: usize, token: &str) -> Result<ClifOperand, ClifSurfaceError> {
    let Some(name) = token.strip_prefix('%') else {
        return Err(error(line, format!("expected a `%` operand, found `{token}`")));
    };
    if name.is_empty() || !name.chars().all(is_identifier_char) {
        return Err(error(line, format!("invalid operand `{token}`")));
    }
    if name.chars().all(|character| character.is_ascii_digit()) {
        let index = name.parse::<usize>().map_err(|_| error(line, format!("invalid parameter index `{token}`")))?;
        return Ok(ClifOperand::Parameter(index));
    }
    if name.starts_with(|character: char| character.is_ascii_digit()) {
        return Err(error(line, format!("local value names must start with a letter or `_`: `{token}`")));
    }
    Ok(ClifOperand::Local(name.to_owned()))
}

fn parse_local_definition(line: usize, token: &str) -> Result<String, ClifSurfaceError> {
    match parse_operand_token(line, token.trim())? {
        ClifOperand::Local(name) => Ok(name),
        ClifOperand::Parameter(index) => Err(error(line, format!("parameter `%{index}` cannot be redefined"))),
    }
}

/// Every `%` operand in `text`, in order.
fn scan_operands(line: usize, text: &str) -> Result<Vec<ClifOperand>, ClifSurfaceError> {
    let mut operands = Vec::new();
    let mut characters = text.char_indices().peekable();
    while let Some((start, character)) = characters.next() {
        if character != '%' {
            continue;
        }
        let mut end = start + 1;
        while let Some(&(position, next)) = characters.peek() {
            if !is_identifier_char(next) {
                break;
            }
            end = position + next.len_utf8();
            characters.next();
        }
        operands.push(parse_operand_token(line, &text[start..end])?);
    }
    Ok(operands)
}

/// Reject raw CLIF entity references that would bypass the `%` naming scheme.
fn reject_raw_entities(line: usize, text: &str) -> Result<(), ClifSurfaceError> {
    if text.contains("->") {
        return Err(error(line, "value aliases (`->`) are not allowed in a clif block"));
    }
    if text.contains('@') {
        return Err(error(line, "`@` is only allowed in `call @symbol(...)`"));
    }
    let mut word = String::new();
    let mut previous_is_percent = false;
    let check = |word: &str| -> Result<(), ClifSurfaceError> {
        for prefix in ["v", "block", "fn", "ss", "dss", "gv", "sig", "jt", "mt", "const", "dt", "ex"] {
            if let Some(rest) = word.strip_prefix(prefix)
                && !rest.is_empty()
                && rest.chars().all(|character| character.is_ascii_digit())
            {
                return Err(error(
                    line,
                    format!("raw CLIF entity `{word}` is not allowed; name values with `%` instead"),
                ));
            }
        }
        Ok(())
    };
    let mut word_after_percent = false;
    for character in text.chars() {
        if is_identifier_char(character) {
            if word.is_empty() {
                word_after_percent = previous_is_percent;
            }
            word.push(character);
        } else {
            if !word.is_empty() && !word_after_percent {
                check(&word)?;
            }
            word.clear();
        }
        previous_is_percent = character == '%';
    }
    if !word.is_empty() && !word_after_percent {
        check(&word)?;
    }
    Ok(())
}

fn parse_statement(line: usize, text: &str) -> Result<ClifStatement, ClifSurfaceError> {
    if let Some(rest) = text.strip_prefix("return")
        && (rest.is_empty() || rest.starts_with(char::is_whitespace))
    {
        let operands = rest.trim();
        if operands.is_empty() {
            return Err(error(line, "`return` requires exactly one value"));
        }
        if operands.contains(',') {
            return Err(error(line, "`return` yields exactly one value"));
        }
        return Ok(ClifStatement::Return { line, value: parse_operand_token(line, operands)? });
    }

    let (results, rhs) = match split_definition(text) {
        Some((lhs, rhs)) => {
            let results = lhs
                .split(',')
                .map(|token| parse_local_definition(line, token))
                .collect::<Result<Vec<_>, _>>()?;
            (results, rhs.trim())
        }
        None => (Vec::new(), text),
    };
    let opcode_token = rhs.split(|character: char| character.is_whitespace()).next().unwrap_or_default();
    let opcode = opcode_token.split('.').next().unwrap_or_default();
    let operand_text = rhs[opcode_token.len()..].trim();

    match opcode {
        "payload" | "length" => {
            if opcode_token != opcode {
                return Err(error(line, format!("`{opcode}` takes no type suffix")));
            }
            let [result] = results.as_slice() else {
                return Err(error(line, format!("`{opcode}` defines exactly one value: `%name = {opcode} %N`")));
            };
            let parameter = match parse_operand_token(line, operand_text)? {
                ClifOperand::Parameter(index) => index,
                ClifOperand::Local(name) => {
                    return Err(error(line, format!("`{opcode}` requires an array parameter `%N`, found `%{name}`")));
                }
            };
            Ok(if opcode == "payload" {
                ClifStatement::Payload { line, result: result.clone(), parameter }
            } else {
                ClifStatement::Length { line, result: result.clone(), parameter }
            })
        }
        "call" => {
            if opcode_token != opcode {
                return Err(error(line, "`call` takes no type suffix; annotate the result with `-> <type>`"));
            }
            if results.len() > 1 {
                return Err(error(line, "`call` defines at most one value"));
            }
            parse_call(line, results.into_iter().next(), operand_text)
        }
        "" => Err(error(line, "expected an instruction")),
        _ => {
            if !CLIF_ALLOWED_OPCODES.contains(&opcode) {
                return Err(error(line, format!("opcode `{opcode}` is not allowed in a clif block")));
            }
            reject_raw_entities(line, rhs)?;
            let operands = scan_operands(line, operand_text)?;
            Ok(ClifStatement::Instruction { line, results, opcode: opcode.to_owned(), text: rhs.to_owned(), operands })
        }
    }
}

/// Split `%a, %b = rhs` at the definition `=`. Only a left side made of `%` names counts.
fn split_definition(text: &str) -> Option<(&str, &str)> {
    let (lhs, rhs) = text.split_once('=')?;
    let lhs_trimmed = lhs.trim();
    (!lhs_trimmed.is_empty()
        && lhs_trimmed
            .split(',')
            .all(|token| token.trim().starts_with('%') && token.trim()[1..].chars().all(is_identifier_char)))
    .then_some((lhs_trimmed, rhs))
}

fn parse_call(line: usize, result: Option<String>, text: &str) -> Result<ClifStatement, ClifSurfaceError> {
    let Some(rest) = text.strip_prefix('@') else {
        return Err(error(line, "expected `call @symbol(...)`"));
    };
    let symbol_end = rest.find(|character: char| !(is_identifier_char(character) || character == '$')).unwrap_or(rest.len());
    let symbol = &rest[..symbol_end];
    if symbol.is_empty() {
        return Err(error(line, "`call` requires a symbol name after `@`"));
    }
    let rest = rest[symbol_end..].trim_start();
    let Some(rest) = rest.strip_prefix('(') else {
        return Err(error(line, format!("expected `(` after `@{symbol}`")));
    };
    let Some(close) = rest.find(')') else {
        return Err(error(line, format!("missing `)` in call to `@{symbol}`")));
    };
    let arguments_text = rest[..close].trim();
    let arguments = if arguments_text.is_empty() {
        Vec::new()
    } else {
        arguments_text
            .split(',')
            .map(|argument| parse_operand_token(line, argument.trim()))
            .collect::<Result<Vec<_>, _>>()?
    };
    let tail = rest[close + 1..].trim();
    let result_type = if tail.is_empty() {
        if result.is_some() {
            return Err(error(line, format!("annotate the result of `@{symbol}`: `%name = call @{symbol}(...) -> i64`")));
        }
        None
    } else if let Some(annotation) = tail.strip_prefix("->") {
        let annotation = annotation.trim();
        if annotation.is_empty() || !annotation.chars().all(is_identifier_char) {
            return Err(error(line, format!("invalid call result type `{annotation}`")));
        }
        Some(annotation.to_owned())
    } else {
        return Err(error(line, format!("unexpected text after call to `@{symbol}`: `{tail}`")));
    };
    Ok(ClifStatement::Call { line, result, symbol: symbol.to_owned(), arguments, result_type })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_forms_parse() {
        let call = parse_clif_surface("call @floor(%0)").expect("legacy call");
        assert!(matches!(
            &call.statements[..],
            [ClifStatement::Call { result: None, symbol, arguments, result_type: None, .. }]
                if symbol == "floor" && arguments == &[ClifOperand::Parameter(0)]
        ));
        let ret = parse_clif_surface("return %0").expect("legacy return");
        assert!(matches!(&ret.statements[..], [ClifStatement::Return { value: ClifOperand::Parameter(0), .. }]));
        let two = parse_clif_surface("call @atan2(%0, %1)").expect("two-argument call");
        assert_eq!(two.value_parameters(), vec![0, 1]);
    }

    #[test]
    fn instruction_block_parses_with_payload() {
        let surface = parse_clif_surface(
            "%p = payload %0\n%n = length %0 // element count\n%w = load.i32 %p+4\n%x = bxor %w, %1 ; mix\nistore32 \
             %x, %p+4\nreturn %x",
        )
        .expect("payload block");
        assert_eq!(surface.array_parameters(), vec![0]);
        assert_eq!(surface.value_parameters(), vec![1]);
        assert!(surface.uses_payload());
    }

    #[test]
    fn rejects_disallowed_opcodes_and_raw_entities() {
        assert!(parse_clif_surface("%x = jump block1\nreturn %x").unwrap_err().message.contains("not allowed"));
        assert!(parse_clif_surface("%x = iadd v0, %1\nreturn %x").unwrap_err().message.contains("raw CLIF entity"));
        assert!(parse_clif_surface("%x = stack_load.i64 ss0\nreturn %x").unwrap_err().message.contains("not allowed"));
    }

    #[test]
    fn rejects_structural_errors() {
        assert!(parse_clif_surface("").is_err());
        assert!(parse_clif_surface("%x = iadd %0, %1").unwrap_err().message.contains("must end with"));
        assert!(parse_clif_surface("return %y").unwrap_err().message.contains("before it is defined"));
        assert!(
            parse_clif_surface("%x = iadd %0, %1\n%x = isub %0, %1\nreturn %x")
                .unwrap_err()
                .message
                .contains("more than once")
        );
        assert!(parse_clif_surface("return %0\nreturn %0").unwrap_err().message.contains("final statement"));
        assert!(
            parse_clif_surface("%p = payload %0\n%r = call @labs(%1) -> i64\nreturn %r")
                .unwrap_err()
                .message
                .contains("safepoint")
        );
        assert!(parse_clif_surface("%p = payload %x\nreturn %p").is_err());
    }

    #[test]
    fn typed_call_parses_result_annotation() {
        let surface = parse_clif_surface("%r = call @labs(%0) -> i64\n%s = iadd %r, %0\nreturn %s").expect("typed call");
        assert!(matches!(
            &surface.statements[0],
            ClifStatement::Call { result: Some(name), result_type: Some(ty), .. } if name == "r" && ty == "i64"
        ));
    }
}
