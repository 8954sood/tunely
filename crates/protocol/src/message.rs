use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMessage {
    RegisterAgent {
        tunnel_id: String,
        token: String,
    },
    RegisterAck {
        ok: bool,
        reason: Option<String>,
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
