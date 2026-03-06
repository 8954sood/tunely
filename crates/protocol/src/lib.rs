pub mod frame;
pub mod message;

pub use frame::{
    ChunkDecodeError, ChunkHeader, StreamKind, WsOpcode, decode_chunk_header, decode_ws_payload,
    encode_chunk_frame, encode_ws_payload,
};
pub use message::ControlMessage;

pub const PROTOCOL_VERSION: u8 = 2;
pub const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;
