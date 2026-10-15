use beskid_serialization::{DataValue, Limits, SequenceKind, validate};

#[test]
fn ieee_bits_are_data_not_numeric_coercion() {
    for bits in [0_u64, 1 << 63, 0x7ff0000000000000, 0xfff0000000000000, 0x7ff8000000000042] {
        let value = DataValue::Float { bits, width: 64 };
        assert_eq!(validate(&value, &Limits::default()), Ok(()));
        assert_eq!(value, DataValue::Float { bits, width: 64 });
    }
    assert!(validate(&DataValue::Float { bits: 1 << 32, width: 32 }, &Limits::default()).is_err());
}

#[test]
fn malformed_widths_optionals_and_duplicate_keys_reject() {
    let limits = Limits::default();
    for value in [
        DataValue::Signed { value: 128, width: 8 },
        DataValue::Unsigned { value: 256, width: 8 },
        DataValue::Optional { present: false, payload: vec![DataValue::Unit] },
        DataValue::Optional { present: true, payload: vec![] },
        DataValue::Map(vec![("x".into(), DataValue::Unit), ("x".into(), DataValue::Unit)]),
    ] {
        assert!(validate(&value, &limits).is_err());
    }
}

#[test]
fn byte_node_and_depth_policies_are_aggregate() {
    let value = DataValue::Sequence {
        kind: SequenceKind::Array,
        values: vec![DataValue::String("é".into()), DataValue::String("é".into())],
    };
    let mut limits = Limits::default();
    limits.max_scalar_bytes = 2;
    limits.max_total_bytes = 3;
    assert!(validate(&value, &limits).is_err());
    limits.max_total_bytes = 4;
    limits.max_nodes = 3;
    limits.max_depth = 1;
    assert_eq!(validate(&value, &limits), Ok(()));
    limits.max_nodes = 2;
    assert!(validate(&value, &limits).is_err());
    limits.max_nodes = 3;
    limits.max_depth = 0;
    assert!(validate(&value, &limits).is_err());
}

#[test]
fn adapters_abort_failed_output_and_validate_decoded_values() {
    use beskid_serialization::{Decoder, Encoder, Error, decode, encode};
    struct Writer { aborted: bool }
    impl Encoder for Writer {
        fn begin(&mut self, _: &Limits) -> Result<(), Error> { Ok(()) }
        fn write_value(&mut self, _: &DataValue) -> Result<(), Error> { Err(Error::Adapter("injected".into())) }
        fn commit(&mut self) -> Result<Vec<u8>, Error> { panic!("failed value must never commit") }
        fn abort(&mut self) { self.aborted = true; }
    }
    let mut writer = Writer { aborted: false };
    assert!(encode(&DataValue::Unit, &mut writer, &Limits::default()).is_err());
    assert!(writer.aborted);
    struct Reader;
    impl Decoder for Reader {
        fn read_value(&mut self, _: &[u8], _: &Limits) -> Result<DataValue, Error> {
            Ok(DataValue::Unsigned { value: 256, width: 8 })
        }
    }
    assert!(decode(&[], &mut Reader, &Limits::default()).is_err());
}
