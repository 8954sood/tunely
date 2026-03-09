use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMessage {
    RegisterAgent {
        tunnel_id: String,
        token: String,
        #[serde(default)]
        request_subdomain: bool,
        #[serde(default)]
        protocol_version: Option<u8>,
        #[serde(default)]
        capabilities: Vec<String>,
    },
    RegisterAck {
        ok: bool,
        reason: Option<String>,
        #[serde(default)]
        subdomain: Option<String>,
        #[serde(default)]
        public_url: Option<String>,
        #[serde(default)]
        protocol_version: Option<u8>,
        #[serde(default)]
        capabilities: Vec<String>,
    },
    HttpRequestStart {
        request_id: Uuid,
        method: String,
        path_and_query: String,
        headers: Vec<(String, String)>,
    },
    HttpRequestEnd {
        request_id: Uuid,
    },
    HttpResponseStart {
        request_id: Uuid,
        status: u16,
        headers: Vec<(String, String)>,
    },
    HttpResponseEnd {
        request_id: Uuid,
    },
    WsConnect {
        stream_id: Uuid,
        path_and_query: String,
        headers: Vec<(String, String)>,
        subprotocols: Vec<String>,
    },
    WsConnectAck {
        stream_id: Uuid,
        ok: bool,
        selected_subprotocol: Option<String>,
        reason: Option<String>,
    },
    WsClose {
        stream_id: Uuid,
        code: Option<u16>,
        reason: Option<String>,
    },
    Error {
        request_id: Option<Uuid>,
        code: String,
        message: String,
    },
    Ping {
        ts_ms: u64,
    },
    Pong {
        ts_ms: u64,
    },
}
