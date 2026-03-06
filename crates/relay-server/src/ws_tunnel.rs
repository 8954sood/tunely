use std::time::Duration;

use axum::{
    body::Body,
    extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
    http::{HeaderMap, Response, StatusCode},
};
use futures_util::{SinkExt, StreamExt};
use protocol::{
    CAP_WS_TUNNEL_V1, ChunkHeader, ControlMessage, StreamKind, WsOpcode,
    encode_chunk_frame_with_version, encode_ws_payload, is_hop_header,
};
use tokio::{sync::oneshot, time::timeout};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::state::{AppState, WsConnectAck, WsRelayEvent};
use crate::ws_wire::send_control;

const WS_CONNECT_TIMEOUT_SECS: u64 = 10;

pub async fn upgrade_client_ws(
    state: AppState,
    tunnel_id: String,
    path_and_query: String,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    let Some(agent) = state.get_agent(&tunnel_id).await else {
        return simple_response(StatusCode::BAD_GATEWAY, "no connected agent for tunnel");
    };
    if !agent.capabilities.contains(CAP_WS_TUNNEL_V1) {
        return simple_response(
            StatusCode::NOT_IMPLEMENTED,
            "connected agent does not support websocket tunneling",
        );
    }

    let stream_id = Uuid::new_v4();
    let forwarded_headers = flatten_ws_headers(&headers);
    let subprotocols = parse_subprotocols(&headers);

    let (ack_tx, ack_rx) = oneshot::channel::<WsConnectAck>();
    state.add_ws_pending(stream_id, tunnel_id.clone(), ack_tx);

    let connect = ControlMessage::WsConnect {
        stream_id,
        path_and_query,
        headers: forwarded_headers,
        subprotocols: subprotocols.clone(),
    };
    if send_control(&agent.sender, &connect).is_err() {
        state.remove_ws_pending(stream_id);
        return simple_response(
            StatusCode::BAD_GATEWAY,
            "failed to notify agent about ws connect",
        );
    }

    let ws = if subprotocols.is_empty() {
        ws
    } else {
        ws.protocols(subprotocols.clone())
    };

    ws.on_upgrade(move |socket| async move {
        handle_client_socket(
            socket,
            state,
            tunnel_id,
            stream_id,
            ack_rx,
            agent.sender,
            agent.protocol_version,
        )
        .await;
    })
}

async fn handle_client_socket(
    socket: WebSocket,
    state: AppState,
    tunnel_id: String,
    stream_id: Uuid,
    ack_rx: oneshot::Receiver<WsConnectAck>,
    agent_sender: tokio::sync::mpsc::UnboundedSender<Message>,
    agent_protocol_version: u8,
) {
    let (mut client_tx, mut client_rx) = socket.split();
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<WsRelayEvent>();
    state.add_ws_stream(stream_id, tunnel_id.clone(), event_tx);

    let ack = match timeout(Duration::from_secs(WS_CONNECT_TIMEOUT_SECS), ack_rx).await {
        Ok(Ok(ack)) => ack,
        Ok(Err(_)) => WsConnectAck {
            ok: false,
            selected_subprotocol: None,
            reason: Some("agent ws connect ack channel closed".to_string()),
        },
        Err(_) => WsConnectAck {
            ok: false,
            selected_subprotocol: None,
            reason: Some("agent ws connect timeout".to_string()),
        },
    };

    if !ack.ok {
        let reason = ack
            .reason
            .unwrap_or_else(|| "agent rejected ws connect".to_string());
        let _ = client_tx
            .send(Message::Close(Some(CloseFrame {
                code: 1011,
                reason: reason.clone().into(),
            })))
            .await;
        state.remove_ws_stream(stream_id);
        state.remove_ws_pending(stream_id);
        warn!(%tunnel_id, %stream_id, %reason, "ws connect rejected");
        return;
    }

    if let Some(protocol) = ack.selected_subprotocol {
        debug!(%tunnel_id, %stream_id, %protocol, "agent selected ws subprotocol");
    }

    loop {
        tokio::select! {
            client = client_rx.next() => {
                let Some(client) = client else {
                    let _ = send_control(&agent_sender, &ControlMessage::WsClose {
                        stream_id,
                        code: Some(1000),
                        reason: Some("client socket ended".to_string()),
                    });
                    break;
                };
                match client {
                    Ok(Message::Text(text)) => {
                        if send_ws_frame(&agent_sender, stream_id, agent_protocol_version, StreamKind::WsClientFrame, WsOpcode::Text, text.as_bytes()).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Binary(bytes)) => {
                        if send_ws_frame(&agent_sender, stream_id, agent_protocol_version, StreamKind::WsClientFrame, WsOpcode::Binary, &bytes).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Ping(bytes)) => {
                        if send_ws_frame(&agent_sender, stream_id, agent_protocol_version, StreamKind::WsClientFrame, WsOpcode::Ping, &bytes).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Pong(bytes)) => {
                        if send_ws_frame(&agent_sender, stream_id, agent_protocol_version, StreamKind::WsClientFrame, WsOpcode::Pong, &bytes).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(close)) => {
                        let (code, reason) = close
                            .map(|c| (Some(c.code), Some(c.reason.to_string())))
                            .unwrap_or((Some(1000), None));
                        let _ = send_control(&agent_sender, &ControlMessage::WsClose {
                            stream_id,
                            code,
                            reason,
                        });
                        break;
                    }
                    Err(err) => {
                        let _ = send_control(&agent_sender, &ControlMessage::WsClose {
                            stream_id,
                            code: Some(1011),
                            reason: Some(format!("client ws read error: {err}")),
                        });
                        break;
                    }
                }
            }
            relay_event = event_rx.recv() => {
                let Some(relay_event) = relay_event else {
                    break;
                };
                match relay_event {
                    WsRelayEvent::Frame { opcode, payload } => {
                        let out = match opcode {
                            WsOpcode::Text => {
                                match std::str::from_utf8(&payload) {
                                    Ok(text) => Message::Text(text.to_string()),
                                    Err(_) => {
                                        warn!(%tunnel_id, %stream_id, "invalid utf8 in ws text payload from agent");
                                        Message::Close(Some(CloseFrame {
                                            code: 1007,
                                            reason: "invalid utf8 payload".into(),
                                        }))
                                    }
                                }
                            }
                            WsOpcode::Binary => Message::Binary(payload.to_vec()),
                            WsOpcode::Ping => Message::Ping(payload.to_vec()),
                            WsOpcode::Pong => Message::Pong(payload.to_vec()),
                            WsOpcode::Close => Message::Close(Some(CloseFrame {
                                code: 1000,
                                reason: "closed by agent".into(),
                            })),
                        };
                        if client_tx.send(out).await.is_err() {
                            break;
                        }
                    }
                    WsRelayEvent::Close { code, reason } => {
                        let _ = client_tx.send(Message::Close(Some(CloseFrame {
                            code: code.unwrap_or(1000),
                            reason: reason.unwrap_or_else(|| "closed".to_string()).into(),
                        }))).await;
                        break;
                    }
                }
            }
        }
    }

    state.remove_ws_stream(stream_id);
    state.remove_ws_pending(stream_id);
}

fn send_ws_frame(
    sender: &tokio::sync::mpsc::UnboundedSender<Message>,
    stream_id: Uuid,
    protocol_version: u8,
    kind: StreamKind,
    opcode: WsOpcode,
    payload: &[u8],
) -> anyhow::Result<()> {
    // Current MVP sends one websocket message per protocol chunk.
    // `seq`/`fin` are reserved for future fragmentation support.
    let frame = encode_chunk_frame_with_version(
        protocol_version,
        ChunkHeader {
            kind,
            request_id: stream_id,
            seq: 0,
            fin: true,
        },
        &encode_ws_payload(opcode, payload),
    );
    sender
        .send(Message::Binary(frame))
        .map_err(|_| anyhow::anyhow!("agent websocket channel closed"))
}

fn flatten_ws_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (name, value) in headers {
        let key = name.as_str();
        if is_hop_header(key) || key.eq_ignore_ascii_case("host") {
            continue;
        }
        if let Ok(value) = value.to_str() {
            out.push((key.to_string(), value.to_string()));
        }
    }
    out
}

fn parse_subprotocols(headers: &HeaderMap) -> Vec<String> {
    let Some(raw) = headers
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok())
    else {
        return Vec::new();
    };

    raw.split(',')
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
        .collect()
}

fn simple_response(status: StatusCode, body: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| Response::new(Body::from("internal error")))
}
