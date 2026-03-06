use thiserror::Error;
use uuid::Uuid;

use crate::PROTOCOL_VERSION;

const HEADER_LEN: usize = 1 + 1 + 16 + 4 + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamKind {
    RequestBody = 0,
    ResponseBody = 1,
    WsClientFrame = 2,
    WsLocalFrame = 3,
}

impl TryFrom<u8> for StreamKind {
    type Error = ChunkDecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::RequestBody),
            1 => Ok(Self::ResponseBody),
            2 => Ok(Self::WsClientFrame),
            3 => Ok(Self::WsLocalFrame),
            _ => Err(ChunkDecodeError::InvalidStreamKind(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WsOpcode {
    Text = 0,
    Binary = 1,
    Ping = 2,
    Pong = 3,
    Close = 4,
}

impl TryFrom<u8> for WsOpcode {
    type Error = ChunkDecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Text),
            1 => Ok(Self::Binary),
            2 => Ok(Self::Ping),
            3 => Ok(Self::Pong),
            4 => Ok(Self::Close),
            _ => Err(ChunkDecodeError::InvalidWsOpcode(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkHeader {
    pub kind: StreamKind,
    pub request_id: Uuid,
    pub seq: u32,
    pub fin: bool,
}

#[derive(Debug, Error)]
pub enum ChunkDecodeError {
    #[error("frame too short")]
    FrameTooShort,
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u8),
    #[error("invalid stream kind: {0}")]
    InvalidStreamKind(u8),
    #[error("missing websocket opcode")]
    MissingWsOpcode,
    #[error("invalid websocket opcode: {0}")]
    InvalidWsOpcode(u8),
}

pub fn encode_chunk_frame(header: ChunkHeader, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.push(PROTOCOL_VERSION);
    out.push(header.kind as u8);
    out.extend_from_slice(header.request_id.as_bytes());
    out.extend_from_slice(&header.seq.to_be_bytes());
    out.push(u8::from(header.fin));
    out.extend_from_slice(payload);
    out
}

pub fn decode_chunk_header(frame: &[u8]) -> Result<(ChunkHeader, &[u8]), ChunkDecodeError> {
    if frame.len() < HEADER_LEN {
        return Err(ChunkDecodeError::FrameTooShort);
    }

    let version = frame[0];
    if version != PROTOCOL_VERSION {
        return Err(ChunkDecodeError::UnsupportedVersion(version));
    }

    let kind = StreamKind::try_from(frame[1])?;

    let mut id_bytes = [0_u8; 16];
    id_bytes.copy_from_slice(&frame[2..18]);
    let request_id = Uuid::from_bytes(id_bytes);

    let mut seq_bytes = [0_u8; 4];
    seq_bytes.copy_from_slice(&frame[18..22]);
    let seq = u32::from_be_bytes(seq_bytes);

    let flags = frame[22];
    let payload = &frame[HEADER_LEN..];

    Ok((
        ChunkHeader {
            kind,
            request_id,
            seq,
            fin: (flags & 0x01) == 0x01,
        },
        payload,
    ))
}

pub fn encode_ws_payload(opcode: WsOpcode, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + payload.len());
    out.push(opcode as u8);
    out.extend_from_slice(payload);
    out
}

pub fn decode_ws_payload(payload: &[u8]) -> Result<(WsOpcode, &[u8]), ChunkDecodeError> {
    let Some((&opcode, rest)) = payload.split_first() else {
        return Err(ChunkDecodeError::MissingWsOpcode);
    };
    let opcode = WsOpcode::try_from(opcode)?;
    Ok((opcode, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_round_trip() {
        let request_id = Uuid::new_v4();
        let header = ChunkHeader {
            kind: StreamKind::ResponseBody,
            request_id,
            seq: 42,
            fin: true,
        };
        let payload = b"hello";

        let encoded = encode_chunk_frame(header, payload);
        let (decoded_header, decoded_payload) = decode_chunk_header(&encoded).expect("decode");

        assert_eq!(decoded_header.kind, StreamKind::ResponseBody);
        assert_eq!(decoded_header.request_id, request_id);
        assert_eq!(decoded_header.seq, 42);
        assert!(decoded_header.fin);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn rejects_short_frame() {
        let err = decode_chunk_header(&[1, 2, 3]).expect_err("must fail");
        assert!(matches!(err, ChunkDecodeError::FrameTooShort));
    }

    #[test]
    fn ws_payload_round_trip() {
        let encoded = encode_ws_payload(WsOpcode::Binary, b"abc");
        let (op, payload) = decode_ws_payload(&encoded).expect("decode");
        assert_eq!(op, WsOpcode::Binary);
        assert_eq!(payload, b"abc");
    }
}
