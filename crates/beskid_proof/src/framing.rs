//! LSP base-protocol framing: `Content-Length: N\r\n\r\n<N bytes of JSON>`.

use std::io::{self, BufRead, Write};

use serde_json::Value;

/// Encode one JSON-RPC message with its header.
pub fn encode(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(body.as_bytes());
    out
}

pub fn write_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    writer.write_all(&encode(message))?;
    writer.flush()
}

/// Read one framed message. `Ok(None)` is a clean end of stream before any header byte.
/// Unknown headers (`Content-Type`) are ignored; a missing or invalid `Content-Length`, a
/// truncated body, or a body that is not JSON is an error.
pub fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut length: Option<usize> = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return if saw_header {
                Err(io::Error::new(io::ErrorKind::UnexpectedEof, "stream ended inside a header"))
            } else {
                Ok(None)
            };
        }
        saw_header = true;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = Some(
                value
                    .trim()
                    .parse()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("bad Content-Length `{value}`")))?,
            );
        }
    }
    let length = length.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header"))?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map(Some).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn encode_uses_byte_length_not_char_length() {
        let message = json!({"text": "za\u{17c}\u{f3}\u{142}\u{107}"});
        let bytes = encode(&message);
        let text = String::from_utf8(bytes.clone()).unwrap();
        let (header, body) = text.split_once("\r\n\r\n").unwrap();
        assert_eq!(header, format!("Content-Length: {}", body.len()));
        assert!(body.len() > body.chars().count());
    }

    #[test]
    fn round_trips_several_messages_in_one_stream() {
        let mut stream = Vec::new();
        let first = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        let second = json!({"jsonrpc": "2.0", "method": "initialized", "params": {}});
        write_message(&mut stream, &first).unwrap();
        write_message(&mut stream, &second).unwrap();
        let mut reader = Cursor::new(stream);
        assert_eq!(read_message(&mut reader).unwrap(), Some(first));
        assert_eq!(read_message(&mut reader).unwrap(), Some(second));
        assert_eq!(read_message(&mut reader).unwrap(), None);
    }

    #[test]
    fn ignores_extra_headers_and_header_case() {
        let raw = b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}";
        assert_eq!(read_message(&mut Cursor::new(&raw[..])).unwrap(), Some(json!({})));
    }

    #[test]
    fn missing_length_is_an_error() {
        let raw = b"Content-Type: x\r\n\r\n{}";
        assert!(read_message(&mut Cursor::new(&raw[..])).is_err());
    }

    #[test]
    fn bad_length_is_an_error() {
        let raw = b"Content-Length: abc\r\n\r\n{}";
        assert!(read_message(&mut Cursor::new(&raw[..])).is_err());
    }

    #[test]
    fn truncated_body_is_an_error() {
        let raw = b"Content-Length: 10\r\n\r\n{}";
        assert!(read_message(&mut Cursor::new(&raw[..])).is_err());
    }

    #[test]
    fn truncated_header_is_an_error_but_empty_stream_is_clean() {
        assert!(read_message(&mut Cursor::new(&b"Content-Len"[..])).is_err());
        assert_eq!(read_message(&mut Cursor::new(&b""[..])).unwrap(), None);
    }

    #[test]
    fn non_json_body_is_an_error() {
        let raw = b"Content-Length: 3\r\n\r\nabc";
        assert!(read_message(&mut Cursor::new(&raw[..])).is_err());
    }
}
