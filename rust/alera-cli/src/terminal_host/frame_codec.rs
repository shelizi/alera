//! Length-prefixed framing for the local terminal-host socket.
//!
//! The default wire format is newline-delimited JSON with PTY output base64'd
//! inside it. That costs the app a UTF-8 decode of the whole socket, a line
//! split, a JSON parse and a base64 decode before the emulator sees a byte,
//! and inflates the traffic by a third.
//!
//! A client that negotiates the binary-frames capability switches the whole
//! connection to uniform frames instead. Uniform, not hybrid: one parser is far
//! easier to get right than a reader that has to tell JSON lines apart from
//! interleaved binary.
//!
//! ```text
//! [u8 kind][u32be length][payload]
//!
//! kind 1 = JSON     payload = the same object that would have been a line
//! kind 2 = output   payload = [u16be len][sessionId utf8][raw pty bytes]
//! ```

use std::io;
use std::sync::OnceLock;

use bytes::Bytes;
use serde_json::Value;

pub const FRAME_KIND_JSON: u8 = 1;
pub const FRAME_KIND_OUTPUT: u8 = 2;

/// Header size: the kind byte plus the u32 length.
pub const FRAME_HEADER_LEN: usize = 5;

pub fn encode_json_frame(value: &Value) -> io::Result<Vec<u8>> {
    encode_json_with_header(value, false)
}

fn encode_json_with_header(value: &Value, newline: bool) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(128);
    out.extend_from_slice(&[FRAME_KIND_JSON, 0, 0, 0, 0]);
    serde_json::to_writer(&mut out, value).map_err(io::Error::other)?;
    let len = u32::try_from(out.len() - FRAME_HEADER_LEN).map_err(io::Error::other)?;
    out[1..FRAME_HEADER_LEN].copy_from_slice(&len.to_be_bytes());
    if newline {
        out.push(b'\n');
    }
    Ok(out)
}

/// A single immutable broadcast, encoded lazily on the first transport writer.
/// The same allocation contains the TCP frame, NDJSON line and bare WebSocket
/// JSON as overlapping views. No work is done for dropped/unread broadcasts.
#[derive(Debug)]
pub struct SharedJsonFrame {
    value: Value,
    encoded: OnceLock<io::Result<Bytes>>,
}

impl SharedJsonFrame {
    pub(crate) fn new(value: Value) -> Self {
        Self {
            value,
            encoded: OnceLock::new(),
        }
    }

    pub(crate) fn value(&self) -> &Value {
        &self.value
    }

    fn encoded(&self) -> io::Result<&Bytes> {
        self.encoded
            .get_or_init(|| encode_json_with_header(&self.value, true).map(Bytes::from))
            .as_ref()
            .map_err(|error| io::Error::new(error.kind(), error.to_string()))
    }

    pub(crate) fn line(&self) -> io::Result<&[u8]> {
        Ok(&self.encoded()?[FRAME_HEADER_LEN..])
    }

    pub(crate) fn binary_frame(&self) -> io::Result<&[u8]> {
        let encoded = self.encoded()?;
        Ok(&encoded[..encoded.len() - 1])
    }

    pub(crate) fn json_payload(&self) -> io::Result<Bytes> {
        let encoded = self.encoded()?;
        Ok(encoded.slice(FRAME_HEADER_LEN..encoded.len() - 1))
    }
}

/// Encodes PTY output without base64, which is the whole point of the format.
pub fn encode_output_frame(session_id: &str, data: &[u8]) -> Vec<u8> {
    let payload_len = 2 + session_id.len() + data.len();
    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + payload_len);
    out.push(FRAME_KIND_OUTPUT);
    out.extend_from_slice(&(payload_len as u32).to_be_bytes());
    append_output_payload(&mut out, session_id, data);
    out
}

/// The output payload on its own, without the length-prefixed header.
///
/// Used by the mobile WebSocket transport, which already frames messages, so
/// it needs the session id and the raw bytes but not a length prefix.
pub fn encode_output_payload(session_id: &str, data: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(2 + session_id.len() + data.len());
    append_output_payload(&mut payload, session_id, data);
    payload
}

fn append_output_payload(payload: &mut Vec<u8>, session_id: &str, data: &[u8]) {
    let id = session_id.as_bytes();
    payload.extend_from_slice(&(id.len() as u16).to_be_bytes());
    payload.extend_from_slice(id);
    payload.extend_from_slice(data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn split_header(bytes: &[u8]) -> (u8, usize, &[u8]) {
        let kind = bytes[0];
        let len = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
        (kind, len, &bytes[FRAME_HEADER_LEN..])
    }

    #[test]
    fn json_frame_round_trips() {
        let value = json!({ "event": "exit", "payload": { "exitCode": 0 } });
        let bytes = encode_json_frame(&value).unwrap();
        let (kind, len, payload) = split_header(&bytes);

        assert_eq!(kind, FRAME_KIND_JSON);
        assert_eq!(len, payload.len());
        assert_eq!(
            serde_json::from_slice::<Value>(payload).unwrap(),
            value,
            "a JSON frame must carry exactly the object a line would have"
        );
    }

    #[test]
    fn shared_json_frame_reuses_one_encoding_across_all_wire_formats() {
        let value = json!({"event": "測試\n", "payload": {"text": "\u{0000}\"\\😀"}});
        let frame = std::sync::Arc::new(SharedJsonFrame::new(value.clone()));
        assert!(frame.encoded.get().is_none());
        let peers = (0..8)
            .map(|_| {
                let frame = frame.clone();
                std::thread::spawn(move || frame.json_payload().unwrap())
            })
            .collect::<Vec<_>>();
        let payload = frame.json_payload().unwrap();
        for peer in peers {
            let peer_payload = peer.join().unwrap();
            assert_eq!(peer_payload.as_ptr(), payload.as_ptr());
            assert_eq!(peer_payload, payload);
        }
        assert_eq!(payload.as_ref(), serde_json::to_vec(&value).unwrap());
        assert_eq!(
            frame.binary_frame().unwrap(),
            encode_json_frame(&value).unwrap()
        );
        assert_eq!(frame.line().unwrap().last(), Some(&b'\n'));
        assert_eq!(&frame.line().unwrap()[..payload.len()], payload.as_ref());
        assert_eq!(frame.line().unwrap().as_ptr(), payload.as_ptr());
    }

    #[test]
    fn output_frame_carries_raw_bytes() {
        let data = vec![0x1b, b'[', b'0', b'm', 0xff, 0x00];
        let bytes = encode_output_frame("session-1", &data);
        let (kind, len, payload) = split_header(&bytes);

        assert_eq!(kind, FRAME_KIND_OUTPUT);
        assert_eq!(len, payload.len());
        let id_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;
        assert_eq!(&payload[2..2 + id_len], b"session-1");
        // Not base64: the raw bytes, including ones that are not valid UTF-8.
        assert_eq!(&payload[2 + id_len..], data.as_slice());
    }

    #[test]
    fn empty_payloads_are_representable() {
        let bytes = encode_output_frame("", &[]);
        let (kind, len, payload) = split_header(&bytes);

        assert_eq!(kind, FRAME_KIND_OUTPUT);
        assert_eq!(len, 2, "just the empty session id length");
        assert_eq!(payload, &[0, 0]);
    }

    #[test]
    fn a_unicode_session_id_is_measured_in_bytes_not_chars() {
        // The length prefix is a byte count, so a multi-byte id must not be
        // truncated by a reader that trusts it.
        let bytes = encode_output_frame("sesión", b"x");
        let (_, _, payload) = split_header(&bytes);
        let id_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;

        assert_eq!(id_len, "sesión".len());
        assert_eq!(
            std::str::from_utf8(&payload[2..2 + id_len]).unwrap(),
            "sesión"
        );
    }
}
