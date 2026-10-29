//! Span-independent structural fingerprint of an expanded syntax tree.
//!
//! The fingerprint walks the tree through its `serde::Serialize` implementation and feeds an
//! unambiguous encoding of every value into one hasher, leaving out each `span` field (struct
//! fields and map entries keyed `span`). No intermediate document is built. Two trees get the same
//! fingerprint exactly when they are equal apart from their spans: the encoding tags every value
//! with its kind, prefixes strings and byte strings with their length and closes every
//! container, so distinct trees never encode to the same byte stream.

use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::Hasher;

use serde::ser::{self, Serialize};

/// Field and map key whose value is source position, not structure.
const SPAN_FIELD: &str = "span";

/// Span-independent fingerprint of `program` under the current grammar revision.
pub(crate) fn expanded_syntax_fingerprint(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> String {
    let mut hasher = DefaultHasher::new();
    let mut encoder = Encoder { sink: &mut hasher, skip_value: false };
    encoder.put_str(beskid_artifacts::grammar_revision());
    program.serialize(&mut encoder).expect("structural encoding is infallible");
    format!("{:016x}", hasher.finish())
}

/// Destination of the encoded byte stream: the fingerprint hasher, or a buffer used to inspect a
/// map key before it is hashed.
trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

impl Sink for DefaultHasher {
    fn put(&mut self, bytes: &[u8]) {
        self.write(bytes);
    }
}

impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

/// Value kind tags of the encoding.
mod tag {
    pub const BOOL: u8 = 1;
    pub const I64: u8 = 2;
    pub const I128: u8 = 3;
    pub const U64: u8 = 4;
    pub const U128: u8 = 5;
    pub const F32: u8 = 6;
    pub const F64: u8 = 7;
    pub const CHAR: u8 = 8;
    pub const STR: u8 = 9;
    pub const BYTES: u8 = 10;
    pub const NONE: u8 = 11;
    pub const SOME: u8 = 12;
    pub const UNIT: u8 = 13;
    pub const UNIT_STRUCT: u8 = 14;
    pub const UNIT_VARIANT: u8 = 15;
    pub const NEWTYPE_STRUCT: u8 = 16;
    pub const NEWTYPE_VARIANT: u8 = 17;
    pub const SEQ: u8 = 18;
    pub const TUPLE: u8 = 19;
    pub const TUPLE_STRUCT: u8 = 20;
    pub const TUPLE_VARIANT: u8 = 21;
    pub const MAP: u8 = 22;
    pub const STRUCT: u8 = 23;
    pub const STRUCT_VARIANT: u8 = 24;
    pub const FIELD: u8 = 25;
    pub const END: u8 = 26;
}

struct Encoder<'a, S: Sink> {
    sink: &'a mut S,
    /// Set by a map key named `span`: the following map value is left out.
    skip_value: bool,
}

impl<S: Sink> Encoder<'_, S> {
    fn put_tag(&mut self, tag: u8) {
        self.sink.put(&[tag]);
    }

    fn put_len(&mut self, len: usize) {
        self.sink.put(&(len as u64).to_le_bytes());
    }

    fn put_str(&mut self, value: &str) {
        self.put_tag(tag::STR);
        self.put_len(value.len());
        self.sink.put(value.as_bytes());
    }

    fn put_variant(&mut self, tag: u8, variant_index: u32, variant: &str) {
        self.put_tag(tag);
        self.sink.put(&variant_index.to_le_bytes());
        self.put_str(variant);
    }
}

/// Encoding errors cannot happen; the type only satisfies the `Serializer` contract.
#[derive(Debug)]
pub(crate) struct EncodeError(String);

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for EncodeError {}

impl ser::Error for EncodeError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

type Result = std::result::Result<(), EncodeError>;

impl<'s, 'a, S: Sink> ser::Serializer for &'s mut Encoder<'a, S> {
    type Ok = ();
    type Error = EncodeError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, value: bool) -> Result {
        self.sink.put(&[tag::BOOL, value as u8]);
        Ok(())
    }

    fn serialize_i8(self, value: i8) -> Result {
        self.serialize_i64(value.into())
    }

    fn serialize_i16(self, value: i16) -> Result {
        self.serialize_i64(value.into())
    }

    fn serialize_i32(self, value: i32) -> Result {
        self.serialize_i64(value.into())
    }

    fn serialize_i64(self, value: i64) -> Result {
        self.put_tag(tag::I64);
        self.sink.put(&value.to_le_bytes());
        Ok(())
    }

    fn serialize_i128(self, value: i128) -> Result {
        self.put_tag(tag::I128);
        self.sink.put(&value.to_le_bytes());
        Ok(())
    }

    fn serialize_u8(self, value: u8) -> Result {
        self.serialize_u64(value.into())
    }

    fn serialize_u16(self, value: u16) -> Result {
        self.serialize_u64(value.into())
    }

    fn serialize_u32(self, value: u32) -> Result {
        self.serialize_u64(value.into())
    }

    fn serialize_u64(self, value: u64) -> Result {
        self.put_tag(tag::U64);
        self.sink.put(&value.to_le_bytes());
        Ok(())
    }

    fn serialize_u128(self, value: u128) -> Result {
        self.put_tag(tag::U128);
        self.sink.put(&value.to_le_bytes());
        Ok(())
    }

    fn serialize_f32(self, value: f32) -> Result {
        self.put_tag(tag::F32);
        self.sink.put(&value.to_bits().to_le_bytes());
        Ok(())
    }

    fn serialize_f64(self, value: f64) -> Result {
        self.put_tag(tag::F64);
        self.sink.put(&value.to_bits().to_le_bytes());
        Ok(())
    }

    fn serialize_char(self, value: char) -> Result {
        self.put_tag(tag::CHAR);
        self.sink.put(&u32::from(value).to_le_bytes());
        Ok(())
    }

    fn serialize_str(self, value: &str) -> Result {
        self.put_str(value);
        Ok(())
    }

    fn serialize_bytes(self, value: &[u8]) -> Result {
        self.put_tag(tag::BYTES);
        self.put_len(value.len());
        self.sink.put(value);
        Ok(())
    }

    fn serialize_none(self) -> Result {
        self.put_tag(tag::NONE);
        Ok(())
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result {
        self.put_tag(tag::SOME);
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result {
        self.put_tag(tag::UNIT);
        Ok(())
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result {
        self.put_tag(tag::UNIT_STRUCT);
        self.put_str(name);
        Ok(())
    }

    fn serialize_unit_variant(self, _name: &'static str, variant_index: u32, variant: &'static str) -> Result {
        self.put_variant(tag::UNIT_VARIANT, variant_index, variant);
        Ok(())
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, name: &'static str, value: &T) -> Result {
        self.put_tag(tag::NEWTYPE_STRUCT);
        self.put_str(name);
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result {
        self.put_variant(tag::NEWTYPE_VARIANT, variant_index, variant);
        value.serialize(self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> std::result::Result<Self, EncodeError> {
        self.put_tag(tag::SEQ);
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> std::result::Result<Self, EncodeError> {
        self.put_tag(tag::TUPLE);
        Ok(self)
    }

    fn serialize_tuple_struct(self, name: &'static str, _len: usize) -> std::result::Result<Self, EncodeError> {
        self.put_tag(tag::TUPLE_STRUCT);
        self.put_str(name);
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> std::result::Result<Self, EncodeError> {
        self.put_variant(tag::TUPLE_VARIANT, variant_index, variant);
        Ok(self)
    }

    fn serialize_map(self, _len: Option<usize>) -> std::result::Result<Self, EncodeError> {
        self.put_tag(tag::MAP);
        Ok(self)
    }

    fn serialize_struct(self, name: &'static str, _len: usize) -> std::result::Result<Self, EncodeError> {
        self.put_tag(tag::STRUCT);
        self.put_str(name);
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> std::result::Result<Self, EncodeError> {
        self.put_variant(tag::STRUCT_VARIANT, variant_index, variant);
        Ok(self)
    }
}

impl<S: Sink> Encoder<'_, S> {
    fn element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result {
        value.serialize(&mut *self)
    }

    fn field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Result {
        if key == SPAN_FIELD {
            return Ok(());
        }
        self.put_tag(tag::FIELD);
        self.put_str(key);
        value.serialize(&mut *self)
    }

    fn end(&mut self) -> Result {
        self.put_tag(tag::END);
        Ok(())
    }
}

macro_rules! sequence_impl {
    ($trait:ident, $method:ident) => {
        impl<S: Sink> ser::$trait for &mut Encoder<'_, S> {
            type Ok = ();
            type Error = EncodeError;

            fn $method<T: ?Sized + Serialize>(&mut self, value: &T) -> Result {
                self.element(value)
            }

            fn end(self) -> Result {
                Encoder::end(self)
            }
        }
    };
}

sequence_impl!(SerializeSeq, serialize_element);
sequence_impl!(SerializeTuple, serialize_element);
sequence_impl!(SerializeTupleStruct, serialize_field);
sequence_impl!(SerializeTupleVariant, serialize_field);

macro_rules! fields_impl {
    ($trait:ident) => {
        impl<S: Sink> ser::$trait for &mut Encoder<'_, S> {
            type Ok = ();
            type Error = EncodeError;

            fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Result {
                self.field(key, value)
            }

            fn end(self) -> Result {
                Encoder::end(self)
            }
        }
    };
}

fields_impl!(SerializeStruct);
fields_impl!(SerializeStructVariant);

impl<S: Sink> ser::SerializeMap for &mut Encoder<'_, S> {
    type Ok = ();
    type Error = EncodeError;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result {
        let mut encoded = Vec::new();
        key.serialize(&mut Encoder { sink: &mut encoded, skip_value: false })?;
        if is_encoded_str(&encoded, SPAN_FIELD) {
            self.skip_value = true;
            return Ok(());
        }
        self.sink.put(&encoded);
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result {
        if std::mem::take(&mut self.skip_value) {
            return Ok(());
        }
        value.serialize(&mut **self)
    }

    fn end(self) -> Result {
        Encoder::end(self)
    }
}

/// Whether `encoded` is the encoding of the string `value`.
fn is_encoded_str(encoded: &[u8], value: &str) -> bool {
    let mut expected = Vec::with_capacity(value.len() + 9);
    Encoder { sink: &mut expected, skip_value: false }.put_str(value);
    encoded == expected.as_slice()
}

#[cfg(test)]
mod tests {
    use super::expanded_syntax_fingerprint;

    fn parse(source: &str) -> beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program> {
        beskid_analysis::services::parse_program(source).expect("fixture parses")
    }

    /// The fingerprint this module replaces: JSON with every `span` key removed. Kept here as the
    /// reference for the change-detection semantics.
    fn json_fingerprint(program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>) -> String {
        fn strip(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Array(values) => values.iter_mut().for_each(strip),
                serde_json::Value::Object(fields) => {
                    fields.remove("span");
                    fields.values_mut().for_each(strip);
                }
                _ => {}
            }
        }
        let mut value = serde_json::to_value(program).unwrap();
        strip(&mut value);
        beskid_artifacts::content_fingerprint(&serde_json::to_string(&value).unwrap())
    }

    const BASE: &str =
        "type Point { i64 x; i64 y; }\ni64 Main() { Point p = Point { x: 1_i64, y: 2_i64 }; return p.x + 42_i64; }";

    #[test]
    fn equal_trees_have_equal_fingerprints() {
        assert_eq!(expanded_syntax_fingerprint(&parse(BASE)), expanded_syntax_fingerprint(&parse(BASE)));
    }

    #[test]
    fn a_span_only_change_keeps_the_fingerprint() {
        let moved = format!("\n\n   // leading comment\n{}", BASE.replace("; ", ";\n      ").replace("{ ", "{\n  "));
        let (base, moved) = (parse(BASE), parse(&moved));
        assert_ne!(base, moved, "the fixture must change spans");
        assert_eq!(expanded_syntax_fingerprint(&base), expanded_syntax_fingerprint(&moved));
    }

    #[test]
    fn a_semantic_change_alters_the_fingerprint() {
        let base = expanded_syntax_fingerprint(&parse(BASE));
        for edited in [
            BASE.replace("42_i64", "43_i64"),
            BASE.replace("p.x +", "p.y +"),
            BASE.replace("Main", "Start"),
            BASE.replace("p.x + 42_i64", "p.x - 42_i64"),
            BASE.replace("i64 y;", "i64 y; i64 z;"),
            BASE.replace("type Point", "pub type Point"),
        ] {
            assert_ne!(base, expanded_syntax_fingerprint(&parse(&edited)), "{edited}");
        }
    }

    /// Agreement with the JSON reference on every pair of fixtures: equal exactly when the JSON
    /// fingerprints are equal.
    #[test]
    fn change_detection_matches_the_span_stripped_json_reference() {
        let sources = [
            BASE.to_owned(),
            format!("  {BASE}\n"),
            BASE.replace("42_i64", "43_i64"),
            BASE.replace("Point p", "Point q").replace("p.x", "q.x"),
            "i64 Main() { return 0_i64; }".to_owned(),
            "i64 Main()\n{\n    return 0_i64;\n}\n".to_owned(),
            "enum Shape { Circle(i64 radius), Square(i64 side) } i64 Main() { return 1_i64; }".to_owned(),
        ];
        let programs: Vec<_> = sources.iter().map(|source| parse(source)).collect();
        for left in &programs {
            for right in &programs {
                assert_eq!(
                    expanded_syntax_fingerprint(left) == expanded_syntax_fingerprint(right),
                    json_fingerprint(left) == json_fingerprint(right)
                );
            }
        }
    }
}
