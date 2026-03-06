pub mod frame;
pub mod http;
pub mod message;

pub use frame::{
    ChunkDecodeError, ChunkHeader, StreamKind, WsOpcode, decode_chunk_header, decode_ws_payload,
    encode_chunk_frame, encode_chunk_frame_with_version, encode_ws_payload,
};
pub use http::is_hop_header;
pub use message::ControlMessage;

pub const LEGACY_PROTOCOL_VERSION: u8 = 1;
pub const PROTOCOL_VERSION: u8 = 2;
pub const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;
pub const CAP_WS_TUNNEL_V1: &str = "ws_tunnel_v1";

pub fn is_supported_protocol_version(version: u8) -> bool {
    version == LEGACY_PROTOCOL_VERSION || version == PROTOCOL_VERSION
}
