use std::{collections::HashSet, time::Duration};

use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    CAP_WS_TUNNEL_V1, ControlMessage, LEGACY_PROTOCOL_VERSION, PROTOCOL_VERSION, StreamKind,
    decode_chunk_header, decode_ws_payload, is_supported_protocol_version,
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::state::{AgentHandle, AppState, is_valid_tunnel_id};
use crate::subdomain::is_valid_dns_label;
use crate::ws_wire::send_control;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    let writer_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let Some(register_msg) = read_register_message(&mut ws_receiver).await else {
        writer_task.abort();
        return;
    };

    let (tunnel_id, token, request_subdomain, protocol_version, capabilities) = match register_msg {
        ControlMessage::RegisterAgent {
            tunnel_id,
            token,
            request_subdomain,
            protocol_version,
            capabilities,
            ..
        } => (
            tunnel_id,
            token,
            request_subdomain,
            protocol_version,
            capabilities,
        ),
        _ => {
            let _ = send_control(
                &out_tx,
                &ControlMessage::RegisterAck {
                    ok: false,
                    reason: Some("first message must be register_agent".to_string()),
                    subdomain: None,
                    public_url: None,
                    protocol_version: Some(PROTOCOL_VERSION),
                    capabilities: relay_capabilities(),
                },
            );
            return;
        }
    };
    let peer_protocol_version = protocol_version.unwrap_or(LEGACY_PROTOCOL_VERSION);
    debug!(
        %tunnel_id,
        request_subdomain,
        protocol_version = peer_protocol_version,
        capability_count = capabilities.len(),
        "received register_agent"
    );
    if !is_supported_protocol_version(peer_protocol_version) {
        let _ = send_control(
            &out_tx,
            &ControlMessage::RegisterAck {
                ok: false,
                reason: Some(format!(
                    "unsupported protocol_version: {peer_protocol_version}"
                )),
                subdomain: None,
                public_url: None,
                protocol_version: Some(PROTOCOL_VERSION),
                capabilities: relay_capabilities(),
            },
        );
        warn!(%tunnel_id, protocol_version = peer_protocol_version, "agent register rejected: unsupported protocol_version");
        return;
    }

    if !is_valid_tunnel_id(&tunnel_id) {
        let _ = send_control(
            &out_tx,
            &ControlMessage::RegisterAck {
                ok: false,
                reason: Some("invalid tunnel_id format".to_string()),
                subdomain: None,
                public_url: None,
                protocol_version: Some(PROTOCOL_VERSION),
                capabilities: relay_capabilities(),
            },
        );
        warn!(%tunnel_id, "agent register rejected: invalid tunnel_id format");
        return;
    }

    if !state.validate_token(&token) {
        let _ = send_control(
            &out_tx,
            &ControlMessage::RegisterAck {
                ok: false,
                reason: Some("invalid token".to_string()),
                subdomain: None,
                public_url: None,
                protocol_version: Some(PROTOCOL_VERSION),
                capabilities: relay_capabilities(),
            },
        );
        warn!(%tunnel_id, "agent register rejected: invalid token");
        return;
    }

    let connection_id = Uuid::new_v4();
    let inserted = state
        .insert_agent_if_absent(
            tunnel_id.clone(),
            AgentHandle {
                connection_id,
                sender: out_tx.clone(),
                capabilities: capabilities.into_iter().collect::<HashSet<_>>(),
                protocol_version: peer_protocol_version,
            },
        )
        .await;
    if !inserted {
        let _ = send_control(
            &out_tx,
            &ControlMessage::RegisterAck {
                ok: false,
                reason: Some("tunnel_id already in use".to_string()),
                subdomain: None,
                public_url: None,
                protocol_version: Some(PROTOCOL_VERSION),
                capabilities: relay_capabilities(),
            },
        );
        warn!(%tunnel_id, "agent register rejected: tunnel_id already in use");
        return;
    }

    if request_subdomain && !is_valid_dns_label(&tunnel_id) {
        let _ = send_control(
            &out_tx,
            &ControlMessage::RegisterAck {
                ok: false,
                reason: Some(
                    "invalid tunnel_id for subdomain mode (use lowercase letters, numbers, '-')"
                        .to_string(),
                ),
                subdomain: None,
                public_url: None,
                protocol_version: Some(PROTOCOL_VERSION),
                capabilities: relay_capabilities(),
            },
        );
        state.remove_agent_if(&tunnel_id, connection_id).await;
        return;
    }

    let mut provisioned_subdomain: Option<String> = None;
    let mut provisioned_public_url: Option<String> = None;
    if request_subdomain {
        debug!(%tunnel_id, "subdomain requested by agent; starting provision flow");
        let Some(provisioner) = state.subdomain() else {
            let _ = send_control(
                &out_tx,
                &ControlMessage::RegisterAck {
                    ok: false,
                    reason: Some("dynamic subdomain is not enabled on relay".to_string()),
                    subdomain: None,
                    public_url: None,
                    protocol_version: Some(PROTOCOL_VERSION),
                    capabilities: relay_capabilities(),
                },
            );
            state.remove_agent_if(&tunnel_id, connection_id).await;
            return;
        };
        match provisioner.provision(&tunnel_id).await {
            Ok(provisioned) => {
                debug!(
                    %tunnel_id,
                    host = %provisioned.host,
                    public_url = %provisioned.public_url,
                    "subdomain provision succeeded"
                );
                provisioned_subdomain = Some(provisioned.host);
                provisioned_public_url = Some(provisioned.public_url);
            }
            Err(err) => {
                warn!(error = %err, %tunnel_id, "subdomain provisioning failed");
                let _ = send_control(
                    &out_tx,
                    &ControlMessage::RegisterAck {
                        ok: false,
                        reason: Some(format!("subdomain provisioning failed: {err}")),
                        subdomain: None,
                        public_url: None,
                        protocol_version: Some(PROTOCOL_VERSION),
                        capabilities: relay_capabilities(),
                    },
                );
                state.remove_agent_if(&tunnel_id, connection_id).await;
                return;
            }
        }
    }

    debug!(
        %tunnel_id,
        has_subdomain = provisioned_subdomain.is_some(),
        "sending register ack"
    );
    let _ = send_control(
        &out_tx,
        &ControlMessage::RegisterAck {
            ok: true,
            reason: None,
            subdomain: provisioned_subdomain.clone(),
            public_url: provisioned_public_url.clone(),
            protocol_version: Some(PROTOCOL_VERSION),
            capabilities: relay_capabilities(),
        },
    );

    info!(%tunnel_id, %connection_id, "agent connected");

    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Err(e) = handle_text_message(&state, &out_tx, text).await {
                    warn!(error = %e, %tunnel_id, "text message handling failed");
                }
            }
            Ok(Message::Binary(bytes)) => {
                if let Err(e) = handle_binary_message(&state, bytes.into()).await {
                    warn!(error = %e, %tunnel_id, "binary message handling failed");
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = out_tx.send(Message::Pong(payload));
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => break,
            Err(e) => {
                warn!(error = %e, %tunnel_id, "websocket read failed");
                break;
            }
        }
    }

    state.remove_agent_if(&tunnel_id, connection_id).await;
    if request_subdomain
        && let Some(provisioner) = state.subdomain()
        && let Err(err) = provisioner.deprovision(&tunnel_id).await
    {
        warn!(error = %err, %tunnel_id, "subdomain deprovision failed");
    }
    info!(%tunnel_id, %connection_id, "agent disconnected");
    writer_task.abort();
}

async fn read_register_message(
    ws_receiver: &mut futures_util::stream::SplitStream<WebSocket>,
) -> Option<ControlMessage> {
    match tokio::time::timeout(Duration::from_secs(10), ws_receiver.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str::<ControlMessage>(&text) {
            Ok(msg) => Some(msg),
            Err(e) => {
                warn!(error = %e, "invalid register payload");
                None
            }
        },
        Ok(Some(Ok(_))) => None,
        Ok(Some(Err(e))) => {
            warn!(error = %e, "failed to read register frame");
            None
        }
        Ok(None) => None,
        Err(_) => {
            warn!("register timeout");
            None
        }
    }
}

async fn handle_text_message(
    state: &AppState,
    out_tx: &mpsc::UnboundedSender<Message>,
    text: String,
) -> anyhow::Result<()> {
    let msg: ControlMessage = serde_json::from_str(&text)?;

    match msg {
        ControlMessage::HttpResponseStart {
            request_id,
            status,
            headers,
        } => {
            state.notify_start(request_id, status, headers).await;
        }
        ControlMessage::HttpResponseEnd { request_id } => {
            state.notify_end(request_id).await;
        }
        ControlMessage::WsConnectAck {
            stream_id,
            ok,
            selected_subprotocol,
            reason,
        } => {
            state.notify_ws_connect_ack(stream_id, ok, selected_subprotocol, reason);
        }
        ControlMessage::WsClose {
            stream_id,
            code,
            reason,
        } => {
            state.notify_ws_close(stream_id, code, reason);
        }
        ControlMessage::Error {
            request_id,
            code,
            message,
        } => {
            if let Some(request_id) = request_id {
                state.notify_error(request_id, code, message).await;
            }
        }
        ControlMessage::Ping { ts_ms } => {
            let pong = ControlMessage::Pong { ts_ms };
            let _ = send_control(out_tx, &pong);
        }
        ControlMessage::Pong { .. } => {}
        other => {
            warn!(?other, "unsupported control message from agent");
        }
    }

    Ok(())
}

fn relay_capabilities() -> Vec<String> {
    vec![CAP_WS_TUNNEL_V1.to_string()]
}

async fn handle_binary_message(state: &AppState, bytes: Bytes) -> anyhow::Result<()> {
    let (header, payload) = decode_chunk_header(&bytes)?;
    match header.kind {
        StreamKind::ResponseBody => {
            state
                .notify_body(header.request_id, Bytes::copy_from_slice(payload))
                .await;
            if header.fin {
                state.notify_end(header.request_id).await;
            }
        }
        StreamKind::RequestBody => {
            warn!(request_id = %header.request_id, "relay received unexpected request_body frame");
        }
        StreamKind::WsLocalFrame => {
            let (opcode, data) = decode_ws_payload(payload)?;
            state.notify_ws_frame(header.request_id, opcode, Bytes::copy_from_slice(data));
        }
        StreamKind::WsClientFrame => {
            warn!(request_id = %header.request_id, "relay received unexpected ws_client_frame");
        }
    }

    Ok(())
}
