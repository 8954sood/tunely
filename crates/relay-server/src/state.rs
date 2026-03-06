use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use axum::{body::Bytes, extract::ws::Message};
use dashmap::DashMap;
use protocol::WsOpcode;
use tokio::sync::{RwLock, mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AgentHandle {
    pub connection_id: Uuid,
    pub sender: mpsc::UnboundedSender<Message>,
}

#[derive(Debug)]
pub enum RelayEvent {
    Start {
        status: u16,
        headers: Vec<(String, String)>,
    },
    Body(Bytes),
    End,
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct WsConnectAck {
    pub ok: bool,
    pub selected_subprotocol: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub enum WsRelayEvent {
    Frame {
        opcode: WsOpcode,
        payload: Bytes,
    },
    Close {
        code: Option<u16>,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct WsStreamHandle {
    tunnel_id: String,
    sender: mpsc::UnboundedSender<WsRelayEvent>,
}

#[derive(Debug)]
struct WsPending {
    tunnel_id: String,
    tx: oneshot::Sender<WsConnectAck>,
}

#[derive(Clone)]
pub struct AppState {
    auth_tokens: Arc<HashSet<String>>,
    agents: Arc<RwLock<HashMap<String, AgentHandle>>>,
    inflight: Arc<DashMap<Uuid, mpsc::Sender<RelayEvent>>>,
    ws_pending: Arc<DashMap<Uuid, WsPending>>,
    ws_streams: Arc<DashMap<Uuid, WsStreamHandle>>,
    pub request_timeout_secs: u64,
}

impl AppState {
    pub fn new(auth_tokens: HashSet<String>, request_timeout_secs: u64) -> Self {
        Self {
            auth_tokens: Arc::new(auth_tokens),
            agents: Arc::new(RwLock::new(HashMap::new())),
            inflight: Arc::new(DashMap::new()),
            ws_pending: Arc::new(DashMap::new()),
            ws_streams: Arc::new(DashMap::new()),
            request_timeout_secs,
        }
    }

    pub fn validate_token(&self, token: &str) -> bool {
        !self.auth_tokens.is_empty() && self.auth_tokens.contains(token)
    }

    pub async fn insert_agent_if_absent(&self, tunnel_id: String, handle: AgentHandle) -> bool {
        let mut agents = self.agents.write().await;
        if agents.contains_key(&tunnel_id) {
            return false;
        }
        agents.insert(tunnel_id, handle);
        true
    }

    pub async fn get_agent(&self, tunnel_id: &str) -> Option<AgentHandle> {
        let agents = self.agents.read().await;
        agents.get(tunnel_id).cloned()
    }

    pub async fn remove_agent_if(&self, tunnel_id: &str, connection_id: Uuid) {
        let mut agents = self.agents.write().await;
        let should_remove = agents
            .get(tunnel_id)
            .is_some_and(|existing| existing.connection_id == connection_id);
        if should_remove {
            agents.remove(tunnel_id);
            self.fail_ws_pending_for_tunnel(tunnel_id, "agent disconnected");
            self.close_ws_for_tunnel(tunnel_id, Some(1011), "agent disconnected");
        }
    }

    pub fn add_inflight(&self, request_id: Uuid, sender: mpsc::Sender<RelayEvent>) {
        self.inflight.insert(request_id, sender);
    }

    pub fn remove_inflight(&self, request_id: Uuid) {
        self.inflight.remove(&request_id);
    }

    pub async fn notify_start(
        &self,
        request_id: Uuid,
        status: u16,
        headers: Vec<(String, String)>,
    ) {
        if let Some(sender) = self
            .inflight
            .get(&request_id)
            .map(|entry| entry.value().clone())
        {
            if sender
                .send(RelayEvent::Start { status, headers })
                .await
                .is_err()
            {
                self.inflight.remove(&request_id);
            }
        }
    }

    pub async fn notify_body(&self, request_id: Uuid, bytes: Bytes) {
        if let Some(sender) = self
            .inflight
            .get(&request_id)
            .map(|entry| entry.value().clone())
        {
            if sender.send(RelayEvent::Body(bytes)).await.is_err() {
                self.inflight.remove(&request_id);
            }
        }
    }

    pub async fn notify_end(&self, request_id: Uuid) {
        if let Some((_, sender)) = self.inflight.remove(&request_id) {
            let _ = sender.send(RelayEvent::End).await;
        }
    }

    pub async fn notify_error(&self, request_id: Uuid, code: String, message: String) {
        if let Some((_, sender)) = self.inflight.remove(&request_id) {
            let _ = sender.send(RelayEvent::Error { code, message }).await;
        }
    }

    pub fn add_ws_pending(
        &self,
        stream_id: Uuid,
        tunnel_id: String,
        tx: oneshot::Sender<WsConnectAck>,
    ) {
        self.ws_pending
            .insert(stream_id, WsPending { tunnel_id, tx });
    }

    pub fn remove_ws_pending(&self, stream_id: Uuid) {
        self.ws_pending.remove(&stream_id);
    }

    pub fn notify_ws_connect_ack(
        &self,
        stream_id: Uuid,
        ok: bool,
        selected_subprotocol: Option<String>,
        reason: Option<String>,
    ) {
        if let Some((_, pending)) = self.ws_pending.remove(&stream_id) {
            let _ = pending.tx.send(WsConnectAck {
                ok,
                selected_subprotocol,
                reason,
            });
        }
    }

    pub fn add_ws_stream(
        &self,
        stream_id: Uuid,
        tunnel_id: String,
        sender: mpsc::UnboundedSender<WsRelayEvent>,
    ) {
        self.ws_streams
            .insert(stream_id, WsStreamHandle { tunnel_id, sender });
    }

    pub fn remove_ws_stream(&self, stream_id: Uuid) {
        self.ws_streams.remove(&stream_id);
    }

    pub fn notify_ws_frame(&self, stream_id: Uuid, opcode: WsOpcode, payload: Bytes) {
        if let Some(sender) = self
            .ws_streams
            .get(&stream_id)
            .map(|entry| entry.value().sender.clone())
        {
            if sender
                .send(WsRelayEvent::Frame { opcode, payload })
                .is_err()
            {
                self.ws_streams.remove(&stream_id);
            }
        }
    }

    pub fn notify_ws_close(&self, stream_id: Uuid, code: Option<u16>, reason: Option<String>) {
        if let Some((_, handle)) = self.ws_streams.remove(&stream_id) {
            let _ = handle.sender.send(WsRelayEvent::Close { code, reason });
        }
    }

    fn fail_ws_pending_for_tunnel(&self, tunnel_id: &str, reason: &str) {
        let ids: Vec<Uuid> = self
            .ws_pending
            .iter()
            .filter(|entry| entry.value().tunnel_id == tunnel_id)
            .map(|entry| *entry.key())
            .collect();
        for id in ids {
            if let Some((_, pending)) = self.ws_pending.remove(&id) {
                let _ = pending.tx.send(WsConnectAck {
                    ok: false,
                    selected_subprotocol: None,
                    reason: Some(reason.to_string()),
                });
            }
        }
    }

    fn close_ws_for_tunnel(&self, tunnel_id: &str, code: Option<u16>, reason: &str) {
        let ids: Vec<Uuid> = self
            .ws_streams
            .iter()
            .filter(|entry| entry.value().tunnel_id == tunnel_id)
            .map(|entry| *entry.key())
            .collect();
        for id in ids {
            if let Some((_, handle)) = self.ws_streams.remove(&id) {
                let _ = handle.sender.send(WsRelayEvent::Close {
                    code,
                    reason: Some(reason.to_string()),
                });
            }
        }
    }
}

pub fn is_valid_tunnel_id(tunnel_id: &str) -> bool {
    if tunnel_id.is_empty() || tunnel_id.len() > 64 {
        return false;
    }
    tunnel_id
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

#[cfg(test)]
mod tests {
    use super::is_valid_tunnel_id;

    #[test]
    fn tunnel_id_validation() {
        assert!(is_valid_tunnel_id("demo"));
        assert!(is_valid_tunnel_id("demo_1"));
        assert!(is_valid_tunnel_id("A-1"));
        assert!(!is_valid_tunnel_id(""));
        assert!(!is_valid_tunnel_id("bad/path"));
        assert!(!is_valid_tunnel_id("bad space"));
    }
}
