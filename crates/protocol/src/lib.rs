pub mod frame;
pub mod message;

pub use frame::{decode_chunk_header, encode_chunk_frame, ChunkDecodeError, ChunkHeader, StreamKind};
pub use message::ControlMessage;

pub const PROTOCOL_VERSION: u8 = 1;
pub const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;
