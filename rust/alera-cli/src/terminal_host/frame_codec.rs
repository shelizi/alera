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

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
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

/// One immutable PTY broadcast shared by two or more recipients. Each wire
/// representation is built at most once, on demand by the transport writer.
/// TCP and WebSocket/relay use overlapping views of the same allocations.
#[derive(Debug)]
pub struct SharedOutputFrame {
    session_id: String,
    data: Bytes,
    binary: OnceLock<Bytes>,
    json_line: OnceLock<io::Result<Bytes>>,
}

impl SharedOutputFrame {
    pub(crate) fn new(session_id: String, data: Bytes) -> Self {
        Self {
            session_id,
            data,
            binary: OnceLock::new(),
            json_line: OnceLock::new(),
        }
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn data(&self) -> &Bytes {
        &self.data
    }

    pub(crate) fn binary_frame(&self) -> &Bytes {
        self.binary
            .get_or_init(|| encode_output_frame(&self.session_id, &self.data).into())
    }

    pub(crate) fn binary_payload(&self) -> Bytes {
        self.binary_frame().slice(FRAME_HEADER_LEN..)
    }

    pub(crate) fn json_line(&self) -> io::Result<&Bytes> {
        self.json_line
            .get_or_init(|| encode_output_json_line(&self.session_id, &self.data))
            .as_ref()
            .map_err(|error| io::Error::new(error.kind(), error.to_string()))
    }

    pub(crate) fn json_payload(&self) -> io::Result<Bytes> {
        let line = self.json_line()?;
        Ok(line.slice(..line.len() - 1))
    }
}

/// Write the base64 alphabet directly into the final JSON allocation. Only
/// the session ID is dynamic JSON text and needs serde's escaping.
pub(crate) fn encode_output_json_line(session_id: &str, data: &[u8]) -> io::Result<Bytes> {
    const PREFIX: &[u8] = br#"{"event":"output","payload":{"dataBase64":""#;
    const ID_PREFIX: &[u8] = br#"","sessionId":"#;
    let encoded_len = base64::encoded_len(data.len(), true)
        .ok_or_else(|| io::Error::other("base64 output length overflow"))?;
    let capacity = encoded_len
        .checked_add(session_id.len())
        .and_then(|len| len.checked_add(PREFIX.len() + ID_PREFIX.len() + 5))
        .ok_or_else(|| io::Error::other("JSON output length overflow"))?;
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(PREFIX);
    let start = out.len();
    out.resize(start + encoded_len, 0);
    let written = STANDARD
        .encode_slice(data, &mut out[start..])
        .map_err(io::Error::other)?;
    debug_assert_eq!(written, encoded_len);
    out.extend_from_slice(ID_PREFIX);
    serde_json::to_writer(&mut out, session_id).map_err(io::Error::other)?;
    out.extend_from_slice(b"}}\n");
    Ok(out.into())
}

/// WebSocket and relay carry the same JSON without the TCP line terminator.
pub(crate) fn encode_output_json_payload(session_id: &str, data: &[u8]) -> io::Result<Bytes> {
    let mut line = encode_output_json_line(session_id, data)?;
    line.truncate(line.len() - 1);
    Ok(line)
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
    fn direct_output_json_matches_legacy_for_padding_and_escaped_ids() {
        for id in [
            "",
            "normal-session",
            "\u{4e2d}\u{6587}\"\n\\\u{0000}",
            "\u{0001}\u{001f}",
        ] {
            for len in [0, 1, 2, 3, 4, 5, 63, 64, 65, 65535, 65536, 65537] {
                let data: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
                let expected = crate::terminal_host::protocol::event(
                    "output",
                    json!({"sessionId": id, "dataBase64": crate::terminal_host::protocol::encode_bytes(&data)}),
                );
                let line = encode_output_json_line(id, &data).unwrap();
                assert_eq!(line.last(), Some(&b'\n'));
                assert_eq!(serde_json::from_slice::<Value>(&line).unwrap(), expected);
                let payload = encode_output_json_payload(id, &data).unwrap();
                assert_eq!(payload.as_ref(), &line[..line.len() - 1]);
                assert_eq!(
                    crate::terminal_host::protocol::decode_bytes(
                        expected["payload"].get("dataBase64")
                    )
                    .unwrap(),
                    data
                );
            }
        }
    }

    #[test]
    fn shared_output_encodes_each_wire_format_only_on_demand() {
        let id = "session-中文\"\n\u{0000}";
        let data = Bytes::from_static(b"\x1b[0m\xff\x00abc");
        let frame = SharedOutputFrame::new(id.into(), data.clone());
        assert!(frame.binary.get().is_none());
        assert!(frame.json_line.get().is_none());
        assert_eq!(frame.data().as_ptr(), data.as_ptr());

        let binary = frame.binary_payload();
        assert_eq!(binary.as_ref(), encode_output_payload(id, &data));
        assert_eq!(
            frame.binary_frame().as_ref(),
            encode_output_frame(id, &data)
        );
        assert_eq!(
            binary.as_ptr(),
            frame.binary_frame()[FRAME_HEADER_LEN..].as_ptr()
        );
        // Binary-only recipients never pay for legacy encoding.
        assert!(frame.json_line.get().is_none());

        let json_payload = frame.json_payload().unwrap();
        let expected = crate::terminal_host::protocol::event(
            "output",
            json!({"sessionId": id, "dataBase64": crate::terminal_host::protocol::encode_bytes(&data)}),
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&json_payload).unwrap(),
            expected
        );
        assert_eq!(frame.json_line().unwrap().last(), Some(&b'\n'));
        assert_eq!(frame.json_line().unwrap().as_ptr(), json_payload.as_ptr());
        assert_eq!(
            frame.json_payload().unwrap().as_ptr(),
            json_payload.as_ptr()
        );
    }

    #[test]
    fn shared_output_concurrent_mixed_transports_reuse_wire_allocations() {
        let data = Bytes::from(vec![0xff; 64 * 1024]);
        let frame = std::sync::Arc::new(SharedOutputFrame::new("s".into(), data));
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let peers: Vec<_> = (0..8)
            .map(|index| {
                let frame = frame.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    if index % 2 == 0 {
                        frame.binary_payload()
                    } else {
                        frame.json_payload().unwrap()
                    }
                })
            })
            .collect();
        let binary = frame.binary_payload();
        let json = frame.json_payload().unwrap();
        for (index, peer) in peers.into_iter().enumerate() {
            let result = peer.join().unwrap();
            let expected = if index % 2 == 0 { &binary } else { &json };
            assert_eq!(&result, expected);
            assert_eq!(result.as_ptr(), expected.as_ptr());
        }

        let legacy_only = SharedOutputFrame::new("s".into(), Bytes::new());
        assert!(legacy_only.json_payload().is_ok());
        assert!(legacy_only.binary.get().is_none());
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
