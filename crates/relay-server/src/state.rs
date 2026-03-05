use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use axum::{
    body::Bytes,
    extract::ws::Message,
};
use dashmap::DashMap;
use tokio::sync::{mpsc, RwLock};
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

#[derive(Clone)]
pub struct AppState {
    auth_tokens: Arc<HashSet<String>>,
    agents: Arc<RwLock<HashMap<String, AgentHandle>>>,
    inflight: Arc<DashMap<Uuid, mpsc::Sender<RelayEvent>>>,
    pub request_timeout_secs: u64,
}

impl AppState {
    pub fn new(auth_tokens: HashSet<String>, request_timeout_secs: u64) -> Self {
        Self {
            auth_tokens: Arc::new(auth_tokens),
            agents: Arc::new(RwLock::new(HashMap::new())),
            inflight: Arc::new(DashMap::new()),
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
        }
    }

    pub fn add_inflight(&self, request_id: Uuid, sender: mpsc::Sender<RelayEvent>) {
        self.inflight.insert(request_id, sender);
    }

    pub fn remove_inflight(&self, request_id: Uuid) {
        self.inflight.remove(&request_id);
    }

    pub async fn notify_start(&self, request_id: Uuid, status: u16, headers: Vec<(String, String)>) {
        if let Some(sender) = self.inflight.get(&request_id).map(|entry| entry.value().clone()) {
            if sender.send(RelayEvent::Start { status, headers }).await.is_err() {
                self.inflight.remove(&request_id);
            }
        }
    }

    pub async fn notify_body(&self, request_id: Uuid, bytes: Bytes) {
        if let Some(sender) = self.inflight.get(&request_id).map(|entry| entry.value().clone()) {
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
