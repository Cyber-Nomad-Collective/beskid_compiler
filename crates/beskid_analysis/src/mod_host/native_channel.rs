//! Private framed worker channel serviced synchronously by the authority's owner.
use super::callback_transport::NativeSemanticSession;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};
const LIMIT: usize = 16 * 1024 * 1024;
pub(crate) struct ParentChannel<'a> {
    listener: TcpListener,
    stream: Option<TcpStream>,
    token: String,
    session: NativeSemanticSession<'a>,
    initial: Value,
    input: Vec<u8>,
    output: Vec<u8>,
    sent: usize,
    authenticated: bool,
    completion: Option<Value>,
}
impl<'a> ParentChannel<'a> {
    pub fn new(session: NativeSemanticSession<'a>, token: String, initial: Value) -> Result<Self> {
        if token.len() < 32 || token.len() > 256 {
            bail!("native worker channel token invalid");
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            stream: None,
            token,
            session,
            initial,
            input: Vec::new(),
            output: Vec::new(),
            sent: 0,
            authenticated: false,
            completion: None,
        })
    }
    pub fn address(&self) -> Result<std::net::SocketAddr> {
        Ok(self.listener.local_addr()?)
    }
    pub fn completion(&mut self) -> Result<super::native_bridge::NativeInvocationOutput> {
        Ok(super::native_bridge::NativeInvocationOutput {
            value: self.completion.take().context("native worker exited without completion frame")?,
            compiled_metadata: self.session.take_compiled_metadata(),
        })
    }
    pub fn into_session(self) -> NativeSemanticSession<'a> {
        self.session
    }
    fn enqueue(&mut self, value: &Value) -> Result<()> {
        let bytes = serde_json::to_vec(value)?;
        if bytes.len() > LIMIT {
            bail!("native worker response exceeds frame budget");
        }
        if self.sent != self.output.len() {
            bail!("native worker requested overlapping response");
        }
        self.output.clear();
        self.sent = 0;
        self.output.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
        self.output.extend(bytes);
        Ok(())
    }
    /// Called by the canonical execution scheduler; never blocks on peer I/O.
    pub fn poll(&mut self) -> Result<()> {
        if self.stream.is_none() {
            match self.listener.accept() {
                Ok((stream, address)) => {
                    if !address.ip().is_loopback() {
                        bail!("native channel peer is not local");
                    }
                    stream.set_nonblocking(true)?;
                    self.stream = Some(stream);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => return Err(e.into()),
            }
        }
        let stream = self.stream.as_mut().unwrap();
        if self.sent < self.output.len() {
            match stream.write(&self.output[self.sent..]) {
                Ok(0) => bail!("native channel closed during response"),
                Ok(n) => self.sent += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        let mut bytes = [0u8; 8192];
        match stream.read(&mut bytes) {
            Ok(0) if self.completion.is_none() => bail!("native worker channel closed without completion"),
            Ok(n) => {
                if self.input.len().saturating_add(n) > LIMIT + 4 {
                    bail!("native worker input exceeds frame budget");
                }
                self.input.extend_from_slice(&bytes[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        if self.input.len() < 4 {
            return Ok(());
        }
        let count = u32::from_be_bytes(self.input[..4].try_into()?) as usize;
        if count == 0 || count > LIMIT {
            bail!("native worker frame size invalid");
        }
        if self.input.len() < count + 4 {
            return Ok(());
        }
        let frame: Value = serde_json::from_slice(&self.input[4..count + 4])?;
        self.input.drain(..count + 4);
        if !self.authenticated {
            if frame.get("version") != Some(&json!(2))
                || frame.get("token").and_then(Value::as_str) != Some(self.token.as_str())
                || frame.get("kind") != Some(&json!("Ready"))
            {
                bail!("native worker channel handshake rejected");
            }
            self.authenticated = true;
            self.enqueue(&self.initial.clone())?;
        } else {
            match frame.get("kind").and_then(Value::as_str) {
                Some("Service") => {
                    let request = frame.get("request").context("native service frame request absent")?;
                    let response = self.session.handle_frame(&serde_json::to_vec(request)?)?;
                    self.enqueue(&serde_json::from_slice::<Value>(&response)?)?;
                }
                Some("Complete") => {
                    if self.completion.is_some() {
                        bail!("native worker repeated completion");
                    }
                    self.completion = Some(frame.get("result").context("native completion result absent")?.clone());
                }
                _ => bail!("native worker frame kind invalid"),
            }
        }
        Ok(())
    }
}

pub(crate) fn read_frame(stream: &mut TcpStream) -> Result<Value> {
    let mut header = [0; 4];
    stream.read_exact(&mut header)?;
    let bytes = u32::from_be_bytes(header) as usize;
    if bytes == 0 || bytes > LIMIT {
        bail!("native worker frame exceeds budget");
    }
    let mut payload = vec![0; bytes];
    stream.read_exact(&mut payload)?;
    Ok(serde_json::from_slice(&payload)?)
}
pub(crate) fn write_frame(stream: &mut TcpStream, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() || bytes.len() > LIMIT {
        bail!("native worker frame exceeds budget");
    }
    stream.write_all(&u32::try_from(bytes.len())?.to_be_bytes())?;
    stream.write_all(&bytes)?;
    Ok(())
}
pub(crate) fn connect(address: std::net::SocketAddr, token: &str) -> Result<(TcpStream, Value)> {
    if !address.ip().is_loopback() {
        bail!("native worker channel address is not local");
    }
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    stream.set_read_timeout(Some(Duration::from_secs(300)))?;
    stream.set_write_timeout(Some(Duration::from_secs(300)))?;
    write_frame(&mut stream, &json!({"version":2,"kind":"Ready","token":token}))?;
    let initial = read_frame(&mut stream)?;
    Ok((stream, initial))
}
