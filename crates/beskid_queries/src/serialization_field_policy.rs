//! Host reading of the Serialization Mod's `SerializeField` field policy.
//!
//! The Mod's `Policies.Field` pass owns field policy and rejects malformed,
//! repeated or asymmetric policies before it publishes any contribution. The
//! host needs the same facts for descriptor emission and shape eligibility:
//! the effective wire name, the two skip decisions, the word wire width and the
//! bytes selection. This reader applies the Mod's literal-only rules argument
//! for argument and fails closed on every spelling the Mod rejects, so it is a
//! second reader of one policy, never a second policy.
use crate::{AstNodeKey, Db, SemanticError};
use beskid_analysis::syntax::{Expression, Field, Literal};

/// The attribute the Serialization Mod declares for field policy.
const FIELD_POLICY_ATTRIBUTE: &str = "SerializeField";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationFieldPolicy {
    wire_name: String,
    renamed: bool,
    skip_write: bool,
    skip_read: bool,
    word_width: Option<u8>,
    bytes: bool,
    default_factory: Option<Vec<String>>,
    catchall: bool,
}
impl SerializationFieldPolicy {
    /// The effective wire name: `Name` when present, else the declared name.
    pub fn wire_name(&self) -> &str {
        &self.wire_name
    }
    pub fn skip_write(&self) -> bool {
        self.skip_write
    }
    pub fn skip_read(&self) -> bool {
        self.skip_read
    }
    /// The explicit wire width of `word` values in this field, when declared.
    pub fn word_width(&self) -> Option<u8> {
        self.word_width
    }
    /// The field selects byte operations; its type must be exactly `u8[]`.
    pub fn bytes(&self) -> bool {
        self.bytes
    }
    pub fn catchall(&self) -> bool {
        self.catchall
    }
    /// The `Default` factory route exactly as written, segment by segment. The
    /// generated decoder calls it by this route from the compilation root.
    pub fn default_factory(&self) -> Option<&[String]> {
        self.default_factory.as_deref()
    }
    /// A field skipped in both directions is never on the wire. Its value comes
    /// only from its default factory, so it is not serialized data.
    pub fn absent_from_wire(&self) -> bool {
        self.skip_write && self.skip_read
    }
    /// A record field is required on the wire only when both directions carry
    /// it. A SkipSerialize field is decode-only and a SkipDeserialize field is
    /// encode-only; neither may fail the other direction as missing.
    pub fn required_in_both_directions(&self) -> bool {
        !self.skip_write && !self.skip_read && !self.catchall
    }
}

fn invalid(message: &str) -> SemanticError {
    SemanticError::new(format!("SerializationFieldPolicy: {message}"))
}

/// The Mod's `Policies.Text`: a quoted literal, no interpolation, and only the
/// `\"`, `\\` and `\${` escapes.
fn text(expression: &Expression) -> Result<String, SemanticError> {
    let Expression::Literal(literal) = expression else { return Err(invalid("policy requires a literal string")) };
    let Literal::String(raw) = &literal.node.literal.node else {
        return Err(invalid("policy requires a literal string"));
    };
    let bytes = raw.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
        return Err(invalid("policy requires a quoted string literal"));
    }
    let body = &bytes[1..bytes.len() - 1];
    let mut decoded = Vec::with_capacity(body.len());
    let mut position = 0;
    while position < body.len() {
        let byte = body[position];
        if byte == b'$' && body.get(position + 1) == Some(&b'{') {
            return Err(invalid("policy string cannot interpolate"));
        }
        if byte == b'\\' {
            position += 1;
            match body.get(position) {
                None => return Err(invalid("truncated policy escape")),
                Some(b'"') | Some(b'\\') => decoded.push(body[position]),
                Some(b'$') if body.get(position + 1) == Some(&b'{') => {
                    decoded.extend_from_slice(b"${");
                    position += 1;
                }
                Some(_) => return Err(invalid("unsupported policy escape")),
            }
        } else {
            decoded.push(byte);
        }
        position += 1;
    }
    String::from_utf8(decoded).map_err(|_| invalid("policy string is not UTF-8"))
}

fn boolean(expression: &Expression) -> Result<bool, SemanticError> {
    match expression {
        Expression::Literal(literal) => match &literal.node.literal.node {
            Literal::Bool(value) => Ok(*value),
            _ => Err(invalid("policy requires a boolean literal")),
        },
        _ => Err(invalid("policy requires a boolean literal")),
    }
}

/// The Mod's `Policies.Width`: unsigned decimal 8, 16, 32 or 64 with no suffix,
/// `_u8` or `_i64`.
fn width(expression: &Expression) -> Result<u8, SemanticError> {
    let rejected = || invalid("word width must be 8, 16, 32 or 64");
    let Expression::Literal(literal) = expression else { return Err(rejected()) };
    let Literal::Integer(raw) = &literal.node.literal.node else { return Err(rejected()) };
    let digits = raw.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return Err(invalid("word width requires an unsigned decimal literal"));
    }
    let mut number = 0u32;
    for byte in raw[..digits].bytes() {
        number = number * 10 + u32::from(byte - b'0');
        if number > 64 {
            return Err(rejected());
        }
    }
    if !matches!(&raw[digits..], "" | "_u8" | "_i64") {
        return Err(invalid("word width literal has an unsupported suffix"));
    }
    match number {
        8 | 16 | 32 | 64 => Ok(number as u8),
        _ => Err(rejected()),
    }
}

/// The Mod's `Policies.Factory` path shape: dot-separated identifier segments.
fn factory_path(text: &str) -> Result<Vec<String>, SemanticError> {
    let mut segments = Vec::new();
    for segment in text.split('.') {
        let mut bytes = segment.bytes();
        let Some(first) = bytes.next() else { return Err(invalid("default factory path has an empty segment")) };
        if !(first == b'_' || first.is_ascii_alphabetic()) || !bytes.all(|b| b == b'_' || b.is_ascii_alphanumeric()) {
            return Err(invalid("default factory must be a qualified identifier path"));
        }
        segments.push(segment.to_owned());
    }
    Ok(segments)
}

/// Policy of one current, registered field declaration.
pub fn serialization_field_policy(db: &dyn Db, field: AstNodeKey) -> Result<SerializationFieldPolicy, SemanticError> {
    let syntax = db
        .syntax_unit(field.unit)
        .filter(|syntax| syntax.accepts_key(db, field))
        .ok_or_else(|| SemanticError::new("serialization field policy key is stale or foreign"))?;
    let declaration = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), field.node)
        .and_then(|node| node.of::<Field>())
        .ok_or_else(|| SemanticError::new("serialization field policy key is not a field declaration"))?;
    let mut policy = SerializationFieldPolicy {
        wire_name: declaration.name.node.name.clone(),
        renamed: false,
        skip_write: false,
        skip_read: false,
        word_width: None,
        bytes: false,
        default_factory: None,
        catchall: false,
    };
    let mut selected = false;
    for attribute in declaration.attributes.iter().filter(|a| a.node.name.node.name == FIELD_POLICY_ATTRIBUTE) {
        if std::mem::replace(&mut selected, true) {
            return Err(invalid("duplicate SerializeField policy"));
        }
        let mut seen = Vec::new();
        for argument in &attribute.node.arguments {
            let name = argument.node.name.node.name.as_str();
            if seen.contains(&name) {
                return Err(invalid("duplicate SerializeField argument"));
            }
            seen.push(name);
            let value = &argument.node.value.node;
            match name {
                "Name" => {
                    let text = text(value)?;
                    if text.is_empty() {
                        return Err(invalid("serialized field name cannot be empty"));
                    }
                    policy.wire_name = text;
                    policy.renamed = true;
                }
                "SkipSerialize" => policy.skip_write = boolean(value)?,
                "SkipDeserialize" => policy.skip_read = boolean(value)?,
                "Bytes" => policy.bytes = boolean(value)?,
                "Catchall" => policy.catchall = boolean(value)?,
                "WordWidth" => policy.word_width = Some(width(value)?),
                "Default" => policy.default_factory = Some(factory_path(&text(value)?)?),
                _ => return Err(invalid("unknown SerializeField argument")),
            }
        }
    }
    if policy.catchall
        && (policy.renamed || policy.bytes || policy.word_width.is_some() || policy.default_factory.is_some() || policy.skip_read)
    {
        return Err(SemanticError::new(
            "SerializationCatchall: a catchall field admits only SkipSerialize; it has no wire name, width, bytes, \
             default or skipped read",
        ));
    }
    if policy.skip_read && policy.default_factory.is_none() {
        return Err(invalid("skipped deserialization field requires an exact typed default factory"));
    }
    Ok(policy)
}

/// Payloads are positional: a payload field is on the wire in both directions
/// or skipped in both and built by its default factory. A catchall or an
/// asymmetric skip has no payload position and fails, as in `Policies.Variant`.
pub fn serialization_payload_field_policy(
    db: &dyn Db,
    field: AstNodeKey,
) -> Result<SerializationFieldPolicy, SemanticError> {
    let policy = serialization_field_policy(db, field)?;
    if policy.catchall {
        return Err(SemanticError::new("SerializationCatchall: only a record field can be a catchall"));
    }
    if policy.skip_read != policy.skip_write {
        return Err(invalid("a positional payload field must be skipped in both directions or in neither"));
    }
    if policy.default_factory.is_some() && !policy.skip_read {
        return Err(invalid("a present positional payload field cannot have a missing-value default"));
    }
    Ok(policy)
}
