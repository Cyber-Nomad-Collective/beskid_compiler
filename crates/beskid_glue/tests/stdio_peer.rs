use beskid_glue::{
    codec::{BinaryEncoder, glue_limits},
    peer::{BindingSpec, Peer, PeerPhase, serve},
    wire::{Envelope, MessageKind},
};
use beskid_serialization::DataValue;
use std::io::{Cursor, Read};

fn frame(kind: MessageKind, id: u64, binding: &[u8; 32], shapes: &[u8], value: Option<&DataValue>) -> Vec<u8> {
    let payload = value
        .map(|value| beskid_serialization::encode(value, &mut BinaryEncoder::default(), &glue_limits()).unwrap())
        .unwrap_or_default();
    Envelope { kind, generation: 9, request: id, library: "compiled-peer", binding, shapes, payload: &payload }
        .encode_frame()
        .unwrap()
}
fn spec() -> BindingSpec {
    BindingSpec { identity: [1; 32], shapes: vec![2; 64] }
}
fn ready<'a>(bindings: &'a [BindingSpec]) -> Peer<'a> {
    let mut peer =
        Peer::new("compiled-peer", 9, DataValue::String("target/abi/layout/bindings".into()), bindings).unwrap();
    peer.receive(
        &frame(MessageKind::Handshake, 0, &[0; 32], &[], Some(&DataValue::String("target/abi/layout/bindings".into()))),
        &mut |_, _| panic!("handshake cannot dispatch"),
    )
    .unwrap();
    peer
}
#[test]
fn exact_handshake_and_binding_admission_precede_effects() {
    let bindings = [spec()];
    let mut peer = ready(&bindings);
    let mut calls = 0;
    let request = frame(MessageKind::Request, 1, &[1; 32], &[2; 64], Some(&DataValue::Signed { value: 7, width: 32 }));
    let reply = peer
        .receive(&request, &mut |index, value| {
            calls += 1;
            assert_eq!(index, 0);
            Ok(value)
        })
        .unwrap()
        .unwrap();
    let decoded = Envelope::decode_frame(&reply).unwrap();
    assert_eq!(decoded.kind, MessageKind::Response);
    assert_eq!(decoded.request, 1);
    assert_eq!(calls, 1);
    assert!(
        peer.receive(&request, &mut |_, _| {
            calls += 1;
            Ok(DataValue::Unit)
        })
        .is_err()
    );
    assert_eq!(calls, 1);
    assert_eq!(peer.phase(), PeerPhase::Failed);
    let mut peer = ready(&bindings);
    let forged = frame(MessageKind::Request, 2, &[1; 32], &[3; 64], Some(&DataValue::Unit));
    assert!(peer.receive(&forged, &mut |_, _| panic!("foreign shapes cannot dispatch")).is_err());
}
#[test]
fn malformed_values_and_incompatible_handshake_never_dispatch() {
    let bindings = [spec()];
    let mut peer = ready(&bindings);
    let malformed = Envelope {
        kind: MessageKind::Request,
        generation: 9,
        request: 1,
        library: "compiled-peer",
        binding: &[1; 32],
        shapes: &[2; 64],
        payload: &[255],
    }
    .encode_frame()
    .unwrap();
    assert!(peer.receive(&malformed, &mut |_, _| panic!("malformed value cannot dispatch")).is_err());
    let mut peer = Peer::new("compiled-peer", 9, DataValue::String("expected".into()), &bindings).unwrap();
    assert!(
        peer.receive(
            &frame(MessageKind::Handshake, 0, &[0; 32], &[], Some(&DataValue::String("foreign".into()))),
            &mut |_, _| panic!("incompatible peer cannot dispatch")
        )
        .is_err()
    );
}
#[test]
fn typed_errors_cancel_and_shutdown_are_protocol_frames() {
    let bindings = [spec()];
    let mut peer = ready(&bindings);
    let request = frame(MessageKind::Request, 1, &[1; 32], &[2; 64], Some(&DataValue::Unit));
    let reply = peer.receive(&request, &mut |_, _| Err(DataValue::String("typed failure".into()))).unwrap().unwrap();
    assert_eq!(Envelope::decode_frame(&reply).unwrap().kind, MessageKind::Error);
    assert!(
        peer.receive(&frame(MessageKind::Cancel, 1, &[1; 32], &[2; 64], None), &mut |_, _| panic!(
            "cancel cannot dispatch"
        ))
        .unwrap()
        .is_none()
    );
    let shutdown = peer
        .receive(&frame(MessageKind::Shutdown, 0, &[0; 32], &[], None), &mut |_, _| panic!("shutdown cannot dispatch"))
        .unwrap()
        .unwrap();
    assert_eq!(Envelope::decode_frame(&shutdown).unwrap().kind, MessageKind::Acknowledgement);
    assert_eq!(peer.phase(), PeerPhase::Closed);
    assert!(peer.receive(&request, &mut |_, _| panic!("closed peer cannot dispatch")).is_err());
}
#[test]
fn cancellation_is_bound_to_original_method_and_output_failure_is_terminal() {
    let bindings = [spec(), BindingSpec { identity: [3; 32], shapes: vec![2; 64] }];
    let mut peer = ready(&bindings);
    peer.receive(&frame(MessageKind::Request, 1, &[1; 32], &[2; 64], Some(&DataValue::Unit)), &mut |_, value| {
        Ok(value)
    })
    .unwrap();
    assert!(
        peer.receive(&frame(MessageKind::Cancel, 1, &[3; 32], &[2; 64], None), &mut |_, _| panic!(
            "cancel cannot dispatch"
        ))
        .is_err()
    );
    let mut peer = ready(&bindings);
    let mut calls = 0;
    let request = frame(MessageKind::Request, 1, &[1; 32], &[2; 64], Some(&DataValue::Unit));
    assert!(
        peer.receive(&request, &mut |_, _| {
            calls += 1;
            Ok(DataValue::Unsigned { value: 256, width: 8 })
        })
        .is_err()
    );
    assert!(
        peer.receive(&request, &mut |_, _| {
            calls += 1;
            Ok(DataValue::Unit)
        })
        .is_err()
    );
    assert_eq!(calls, 1);
}
#[test]
fn premature_eof_and_closed_service_cannot_admit_or_wait() {
    let bindings = [spec()];
    let mut peer = ready(&bindings);
    assert!(
        serve(&mut peer, &mut Cursor::new(Vec::<u8>::new()), &mut Vec::new(), &mut |_, _| panic!(
            "EOF cannot dispatch"
        ))
        .is_err()
    );
    let mut peer = ready(&bindings);
    peer.receive(&frame(MessageKind::Shutdown, 0, &[0; 32], &[], None), &mut |_, _| panic!("shutdown cannot dispatch"))
        .unwrap();
    struct ForbiddenRead;
    impl Read for ForbiddenRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("closed peer cannot read again")
        }
    }
    assert!(
        serve(&mut peer, &mut ForbiddenRead, &mut Vec::new(), &mut |_, _| panic!("closed cannot dispatch")).is_err()
    );
}
struct Fragmented(Cursor<Vec<u8>>);
impl Read for Fragmented {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(&mut bytes[..1])
    }
}
#[test]
fn service_drives_partial_frames_and_flushes_ack_before_return() {
    let bindings = [spec()];
    let mut peer = Peer::new("compiled-peer", 9, DataValue::Unit, &bindings).unwrap();
    let mut input = frame(MessageKind::Handshake, 0, &[0; 32], &[], Some(&DataValue::Unit));
    input.extend(frame(MessageKind::Request, 1, &[1; 32], &[2; 64], Some(&DataValue::Bytes(vec![0, 255, 13, 10]))));
    input.extend(frame(MessageKind::Shutdown, 0, &[0; 32], &[], None));
    let mut output = Vec::new();
    serve(&mut peer, &mut Fragmented(Cursor::new(input)), &mut output, &mut |_, value| Ok(value)).unwrap();
    let mut kinds = Vec::new();
    let mut remaining = output.as_slice();
    while !remaining.is_empty() {
        let length = u32::from_le_bytes(remaining[..4].try_into().unwrap()) as usize + 4;
        kinds.push(Envelope::decode_frame(&remaining[..length]).unwrap().kind);
        remaining = &remaining[length..];
    }
    assert_eq!(kinds, [MessageKind::Acknowledgement, MessageKind::Response, MessageKind::Acknowledgement]);
    assert_eq!(peer.phase(), PeerPhase::Closed);
}
