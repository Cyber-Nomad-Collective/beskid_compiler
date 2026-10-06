//! Format-neutral owned values and bounded adapter contracts.
//! A digest carried by a value describes wire identity; it never grants a
//! compiler-issued shape, field, resource, or catchall capability.
use std::{collections::HashSet, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceKind {
    Array,
    List,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataValue {
    Unit,
    Boolean(bool),
    Signed { value: i64, width: u8 },
    Unsigned { value: u64, width: u8 },
    Float { bits: u64, width: u8 },
    Scalar(char),
    String(String),
    Bytes(Vec<u8>),
    Sequence { kind: SequenceKind, values: Vec<DataValue> },
    Record { shape: [u8; 32], fields: Vec<(String, DataValue)> },
    Map(Vec<(String, DataValue)>),
    Variant { shape: [u8; 32], name: String, payload: Vec<DataValue> },
    Optional { present: bool, payload: Vec<DataValue> },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_total_bytes: usize,
    pub max_scalar_bytes: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 8 * 1024 * 1024,
            max_output_bytes: 8 * 1024 * 1024,
            max_total_bytes: 8 * 1024 * 1024,
            max_scalar_bytes: 1024 * 1024,
            max_nodes: 100_000,
            max_depth: 128,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidValue(&'static str),
    Limit(&'static str),
    AllocationFailure,
    Adapter(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Validate an owned tree without recursive Rust calls or numeric coercion.
/// Root depth is zero. All key/name/string/byte payloads debit one shared budget.
pub fn validate(value: &DataValue, limits: &Limits) -> Result<(), Error> {
    if limits.max_nodes == 0 {
        return Err(Error::Limit("nodes"));
    }
    let mut pending = Vec::new();
    pending.try_reserve(1).map_err(|_| Error::AllocationFailure)?;
    pending.push((value, 0usize));
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    let mut scalar = |length: usize| -> Result<(), Error> {
        if length > limits.max_scalar_bytes {
            return Err(Error::Limit("scalar bytes"));
        }
        bytes = bytes.checked_add(length).ok_or(Error::Limit("total bytes"))?;
        if bytes > limits.max_total_bytes {
            return Err(Error::Limit("total bytes"));
        }
        Ok(())
    };
    while let Some((value, depth)) = pending.pop() {
        nodes = nodes.checked_add(1).ok_or(Error::Limit("nodes"))?;
        if nodes > limits.max_nodes {
            return Err(Error::Limit("nodes"));
        }
        if depth > limits.max_depth {
            return Err(Error::Limit("depth"));
        }
        let children: &[DataValue] = match value {
            DataValue::Signed { value, width } => {
                if !matches!(width, 8 | 16 | 32 | 64) {
                    return Err(Error::InvalidValue("signed width"));
                }
                if *width < 64 {
                    let maximum = (1_i64 << (*width - 1)) - 1;
                    if *value < -maximum - 1 || *value > maximum {
                        return Err(Error::InvalidValue("signed range"));
                    }
                }
                &[]
            }
            DataValue::Unsigned { value, width } => {
                if !matches!(width, 8 | 16 | 32 | 64) {
                    return Err(Error::InvalidValue("unsigned width"));
                }
                if *width < 64 && *value >= (1_u64 << *width) {
                    return Err(Error::InvalidValue("unsigned range"));
                }
                &[]
            }
            DataValue::Float { bits, width } => {
                if !matches!(width, 32 | 64) || (*width == 32 && *bits > u32::MAX as u64) {
                    return Err(Error::InvalidValue("float bits/width"));
                }
                &[]
            }
            DataValue::String(value) => {
                scalar(value.len())?;
                &[]
            }
            DataValue::Bytes(value) => {
                scalar(value.len())?;
                &[]
            }
            DataValue::Scalar(value) => {
                scalar(value.len_utf8())?;
                &[]
            }
            DataValue::Sequence { values, .. } => values,
            DataValue::Variant { name, payload, .. } => {
                scalar(name.len())?;
                payload
            }
            DataValue::Optional { present, payload } => {
                if payload.len() != usize::from(*present) {
                    return Err(Error::InvalidValue("optional cardinality"));
                }
                payload
            }
            DataValue::Record { fields, .. } | DataValue::Map(fields) => {
                if fields.len() > limits.max_nodes.saturating_sub(nodes).saturating_sub(pending.len()) {
                    return Err(Error::Limit("nodes"));
                }
                let mut seen = HashSet::new();
                seen.try_reserve(fields.len()).map_err(|_| Error::AllocationFailure)?;
                pending.try_reserve(fields.len()).map_err(|_| Error::AllocationFailure)?;
                let child_depth = depth.checked_add(1).ok_or(Error::Limit("depth"))?;
                for (key, child) in fields {
                    scalar(key.len())?;
                    if !seen.insert(key.as_str()) {
                        return Err(Error::InvalidValue("duplicate field/key"));
                    }
                    pending.push((child, child_depth));
                }
                continue;
            }
            DataValue::Unit | DataValue::Boolean(_) => &[],
        };
        if children.len() > limits.max_nodes.saturating_sub(nodes).saturating_sub(pending.len()) {
            return Err(Error::Limit("nodes"));
        }
        pending.try_reserve(children.len()).map_err(|_| Error::AllocationFailure)?;
        let child_depth = depth.checked_add(1).ok_or(Error::Limit("depth"))?;
        pending.extend(children.iter().rev().map(|child| (child, child_depth)));
    }
    Ok(())
}

/// Adapter-owned unpublished transaction. A failed call must not publish output;
/// abort is idempotent and must not allocate. Commit transfers owned bytes only.
pub trait Encoder {
    fn begin(&mut self, limits: &Limits) -> Result<(), Error>;
    fn write_value(&mut self, value: &DataValue) -> Result<(), Error>;
    fn commit(&mut self) -> Result<Vec<u8>, Error>;
    fn abort(&mut self);
}
/// Decode one complete value; trailing bytes are an error. Adapter allocation is
/// bounded by the supplied policy, before allocating a collection/string.
pub trait Decoder {
    fn read_value(&mut self, input: &[u8], limits: &Limits) -> Result<DataValue, Error>;
}
pub fn encode<E: Encoder>(value: &DataValue, encoder: &mut E, limits: &Limits) -> Result<Vec<u8>, Error> {
    validate(value, limits)?;
    let result = encoder.begin(limits).and_then(|_| encoder.write_value(value)).and_then(|_| encoder.commit());
    match result {
        Ok(bytes) if bytes.len() <= limits.max_output_bytes => Ok(bytes),
        Ok(_) => {
            encoder.abort();
            Err(Error::Limit("output bytes"))
        }
        Err(error) => {
            encoder.abort();
            Err(error)
        }
    }
}
pub fn decode<D: Decoder>(input: &[u8], decoder: &mut D, limits: &Limits) -> Result<DataValue, Error> {
    if input.len() > limits.max_input_bytes {
        return Err(Error::Limit("input bytes"));
    }
    let value = decoder.read_value(input, limits)?;
    validate(&value, limits)?;
    Ok(value)
}
