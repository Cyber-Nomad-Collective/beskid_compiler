//! Reciprocal Rust service driven by the one canonical Glue wire/value adapter.
//! Generated source supplies trusted compatibility data and typed dispatchers;
//! a wire digest never authorizes a native image, pointer or owner registration.
use crate::{
    codec::{BinaryDecoder, BinaryEncoder, glue_limits},
    pump::{Pump, PumpError},
    wire::{Envelope, MessageKind, WireError},
};
use beskid_serialization::{DataValue, Error as ValueError};
use std::io::{self, Read, Write};

pub struct BindingSpec {
    pub identity: [u8; 32],
    pub shapes: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerPhase {
    Handshake,
    Ready,
    Closed,
    Failed,
}
#[derive(Debug)]
pub enum PeerError {
    Wire(WireError),
    Value(ValueError),
    Pump(PumpError),
    Io(io::Error),
    Handshake,
    Binding,
    Request,
    Closed,
    UnexpectedEof,
}
impl From<WireError> for PeerError {
    fn from(value: WireError) -> Self {
        Self::Wire(value)
    }
}
impl From<ValueError> for PeerError {
    fn from(value: ValueError) -> Self {
        Self::Value(value)
    }
}
impl From<PumpError> for PeerError {
    fn from(value: PumpError) -> Self {
        Self::Pump(value)
    }
}
impl From<io::Error> for PeerError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl std::fmt::Display for PeerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PeerError {}

/// No scheduler or native registry lives here. Dispatch runs in the Rust peer's
/// existing service thread; the Beskid owner drives its cooperative pipe pump
/// and can settle cancellation/terminate the process without awaiting a body.
pub struct Peer<'a> {
    library: &'a str,
    generation: u64,
    compatibility: DataValue,
    bindings: &'a [BindingSpec],
    phase: PeerPhase,
    last_request: u64,
    settled: [Option<(u64, usize)>; 256],
    settlement_index: usize,
}
impl<'a> Peer<'a> {
    pub fn new(
        library: &'a str,
        generation: u64,
        compatibility: DataValue,
        bindings: &'a [BindingSpec],
    ) -> Result<Self, PeerError> {
        let payload = beskid_serialization::encode(&compatibility, &mut BinaryEncoder::default(), &glue_limits())?;
        Envelope {
            kind: MessageKind::Handshake,
            generation,
            request: 0,
            library,
            binding: &[0; 32],
            shapes: &[],
            payload: &payload,
        }
        .encode_frame()?;
        if bindings.len() > 4096 {
            return Err(PeerError::Binding);
        }
        for (index, binding) in bindings.iter().enumerate() {
            if binding.identity == [0; 32]
                || binding.shapes.len() % 32 != 0
                || binding.shapes.len() / 32 > 256
                || bindings[..index].iter().any(|previous| previous.identity == binding.identity)
            {
                return Err(PeerError::Binding);
            }
        }
        Ok(Self {
            library,
            generation,
            compatibility,
            bindings,
            phase: PeerPhase::Handshake,
            last_request: 0,
            settled: [None; 256],
            settlement_index: 0,
        })
    }
    pub fn phase(&self) -> PeerPhase {
        self.phase
    }
    pub fn receive<F>(&mut self, frame: &[u8], dispatch: &mut F) -> Result<Option<Vec<u8>>, PeerError>
    where
        F: FnMut(usize, DataValue) -> Result<DataValue, DataValue>,
    {
        if matches!(self.phase, PeerPhase::Closed | PeerPhase::Failed) {
            return Err(PeerError::Closed);
        }
        let result = self.receive_inner(frame, dispatch);
        if result.is_err() {
            self.phase = PeerPhase::Failed;
        }
        result
    }
    fn receive_inner<F>(&mut self, frame: &[u8], dispatch: &mut F) -> Result<Option<Vec<u8>>, PeerError>
    where
        F: FnMut(usize, DataValue) -> Result<DataValue, DataValue>,
    {
        let envelope = Envelope::decode_frame(frame)?;
        if envelope.library != self.library || envelope.generation != self.generation {
            return Err(PeerError::Handshake);
        }
        if self.phase == PeerPhase::Handshake {
            if envelope.kind != MessageKind::Handshake {
                return Err(PeerError::Handshake);
            }
            let compatibility = beskid_serialization::decode(envelope.payload, &mut BinaryDecoder, &glue_limits())?;
            if compatibility != self.compatibility {
                return Err(PeerError::Handshake);
            }
            let payload =
                beskid_serialization::encode(&self.compatibility, &mut BinaryEncoder::default(), &glue_limits())?;
            let acknowledgement = self.reply(MessageKind::Acknowledgement, 0, &[0; 32], &[], &payload)?;
            self.phase = PeerPhase::Ready;
            return Ok(Some(acknowledgement));
        }
        if envelope.kind == MessageKind::Shutdown {
            if !envelope.payload.is_empty() {
                return Err(PeerError::Request);
            }
            let acknowledgement = self.reply(MessageKind::Acknowledgement, 0, &[0; 32], &[], &[])?;
            self.phase = PeerPhase::Closed;
            return Ok(Some(acknowledgement));
        }
        if !matches!(envelope.kind, MessageKind::Request | MessageKind::Cancel) {
            return Err(PeerError::Request);
        }
        let index =
            self.bindings.iter().position(|binding| binding.identity == *envelope.binding).ok_or(PeerError::Binding)?;
        if self.bindings[index].shapes != envelope.shapes {
            return Err(PeerError::Binding);
        }
        if envelope.kind == MessageKind::Cancel {
            // Bodies are serialized in this peer. The owner may cancel its
            // waiter before our response arrives; cancellation acknowledges no
            // new call and must still match the already dispatched identity.
            if !envelope.payload.is_empty()
                || !self.settled.iter().any(|entry| *entry == Some((envelope.request, index)))
            {
                return Err(PeerError::Request);
            }
            return Ok(None);
        }
        if envelope.request <= self.last_request {
            return Err(PeerError::Request);
        }
        let value = beskid_serialization::decode(envelope.payload, &mut BinaryDecoder, &glue_limits())?;
        // Consume before effects: failures or duplicate frames cannot dispatch
        // the same request again, including output encoding failure.
        self.last_request = envelope.request;
        self.settled[self.settlement_index] = Some((envelope.request, index));
        self.settlement_index = (self.settlement_index + 1) % self.settled.len();
        let (kind, output) = match dispatch(index, value) {
            Ok(value) => (MessageKind::Response, value),
            Err(error) => (MessageKind::Error, error),
        };
        let payload = beskid_serialization::encode(&output, &mut BinaryEncoder::default(), &glue_limits())?;
        Ok(Some(self.reply(kind, envelope.request, envelope.binding, envelope.shapes, &payload)?))
    }
    fn reply(
        &self,
        kind: MessageKind,
        request: u64,
        binding: &[u8; 32],
        shapes: &[u8],
        payload: &[u8],
    ) -> Result<Vec<u8>, PeerError> {
        Ok(Envelope { kind, generation: self.generation, request, library: self.library, binding, shapes, payload }
            .encode_frame()?)
    }
}

/// Drive the peer's existing standard streams. A bounded read buffer and Pump
/// cover arbitrary fragmentation/coalescing. This function emits frames only;
/// generated main reports terminal diagnostics on stderr. Rust-owned values
/// drop normally on every return, and the parent owns process termination/reap.
pub fn serve<R: Read, W: Write, F>(
    peer: &mut Peer<'_>,
    input: &mut R,
    output: &mut W,
    dispatch: &mut F,
) -> Result<(), PeerError>
where
    F: FnMut(usize, DataValue) -> Result<DataValue, DataValue>,
{
    let result = serve_inner(peer, input, output, dispatch);
    if result.is_err() && peer.phase != PeerPhase::Closed {
        peer.phase = PeerPhase::Failed;
    }
    result
}
fn serve_inner<R: Read, W: Write, F>(
    peer: &mut Peer<'_>,
    input: &mut R,
    output: &mut W,
    dispatch: &mut F,
) -> Result<(), PeerError>
where
    F: FnMut(usize, DataValue) -> Result<DataValue, DataValue>,
{
    if matches!(peer.phase, PeerPhase::Closed | PeerPhase::Failed) {
        return Err(PeerError::Closed);
    }
    let mut pump = Pump::default();
    let mut bytes = [0u8; 8192];
    loop {
        let count = match input.read(&mut bytes) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            value => value?,
        };
        if count == 0 {
            pump.eof()?;
            return Err(PeerError::UnexpectedEof);
        }
        let mut position = 0;
        while position < count {
            let (consumed, frame) = pump.feed(&bytes[position..count])?;
            position += consumed;
            if let Some(frame) = frame {
                if let Some(reply) = peer.receive(&frame, dispatch)? {
                    output.write_all(&reply)?;
                    output.flush()?;
                }
                if peer.phase == PeerPhase::Closed {
                    return Ok(());
                }
            }
        }
    }
}
pub fn serve_stdio<F>(peer: &mut Peer<'_>, dispatch: &mut F) -> Result<(), PeerError>
where
    F: FnMut(usize, DataValue) -> Result<DataValue, DataValue>,
{
    serve(peer, &mut io::stdin().lock(), &mut io::stdout().lock(), dispatch)
}
