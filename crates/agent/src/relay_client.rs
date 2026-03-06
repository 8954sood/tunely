use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    ChunkHeader, ControlMessage, StreamKind, WsOpcode, decode_chunk_header, decode_ws_payload,
    encode_chunk_frame, encode_ws_payload,
};
use rand::Rng;
use tokio::{
    sync::Mutex,
    sync::mpsc,
    time::{interval, timeout},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue},
    },
};
use tracing::{error, info, warn};
use url::Url;
use uuid::Uuid;

use crate::{
    config::Config,
    inflight::{BodyReceiver, Inflight},
    local_proxy::{
        apply_forward_headers, compose_local_url, compose_local_ws_url, flatten_response_headers,
        should_skip_ws_forward_header,
    },
};

#[derive(Debug)]
enum WsInboundEvent {
    Frame {
        opcode: WsOpcode,
        payload: Bytes,
    },
    Close {
        code: Option<u16>,
        reason: Option<String>,
    },
}

type WsInboundSender = mpsc::UnboundedSender<WsInboundEvent>;
type WsSessionMap = Arc<Mutex<HashMap<Uuid, WsInboundSender>>>;

pub async fn run(config: Config) -> anyhow::Result<()> {
    let mut attempt: u32 = 0;

    loop {
        match run_once(&config).await {
            Ok(()) => {
                attempt = 0;
                warn!("relay connection ended; reconnecting");
            }
            Err(err) => {
                warn!(error = %err, "relay connection failed");
                attempt = attempt.saturating_add(1);
            }
        }

        let sleep_for = backoff_with_jitter(attempt, config.max_backoff_secs);
        info!(delay_ms = sleep_for.as_millis(), "reconnecting to relay");
        tokio::time::sleep(sleep_for).await;
    }
}

async fn run_once(config: &Config) -> anyhow::Result<()> {
    let (ws_stream, _) = connect_async(&config.relay).await?;
    let (mut ws_writer, mut ws_reader) = ws_stream.split();

    send_register(&mut ws_writer, config).await?;
    wait_register_ack(&mut ws_reader).await?;
    if let Some(public_url) = derive_public_tunnel_url(&config.relay, &config.tunnel_id) {
        info!(tunnel_id = %config.tunnel_id, %public_url, "agent registered; client access URL");
    } else {
        info!(tunnel_id = %config.tunnel_id, "agent registered");
    }

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let writer_task = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if ws_writer.send(message).await.is_err() {
                break;
            }
        }
    });

    let ping_tx = out_tx.clone();
    let ping_interval = config.ping_interval_secs;
    let ping_task = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(ping_interval));
        loop {
            ticker.tick().await;
            let ping = ControlMessage::Ping { ts_ms: now_ms() };
            if send_control(&ping_tx, &ping).is_err() {
                break;
            }
        }
    });

    let client = reqwest::Client::new();
    let mut inflight = Inflight::default();
    let ws_sessions: WsSessionMap = Arc::new(Mutex::new(HashMap::new()));

    while let Some(msg) = ws_reader.next().await {
        let msg = msg?;
        match msg {
            Message::Text(text) => {
                handle_text_message(
                    text,
                    &mut inflight,
                    ws_sessions.clone(),
                    &out_tx,
                    &client,
                    config.local.clone(),
                )
                .await;
            }
            Message::Binary(bytes) => {
                handle_binary_message(bytes.into(), &mut inflight, ws_sessions.clone()).await;
            }
            Message::Ping(payload) => {
                let _ = out_tx.send(Message::Pong(payload));
            }
            Message::Pong(_) => {}
            Message::Close(_) => break,
            Message::Frame(_) => {}
        }
    }

    inflight.clear();
    ws_sessions.lock().await.clear();
    writer_task.abort();
    ping_task.abort();
    Ok(())
}

async fn send_register(
    ws_writer: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    config: &Config,
) -> anyhow::Result<()> {
    let register = ControlMessage::RegisterAgent {
        tunnel_id: config.tunnel_id.clone(),
        token: config.token.clone(),
    };
    ws_writer
        .send(Message::Text(serde_json::to_string(&register)?))
        .await?;
    Ok(())
}

async fn wait_register_ack(
    ws_reader: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> anyhow::Result<()> {
    let maybe_msg = timeout(Duration::from_secs(10), ws_reader.next()).await?;
    let frame_result =
        maybe_msg.ok_or_else(|| anyhow::anyhow!("relay closed websocket before register ack"))?;
    let msg = frame_result?;

    let Message::Text(text) = msg else {
        anyhow::bail!("register ack must be text frame")
    };

    let ack: ControlMessage = serde_json::from_str(&text)?;
    match ack {
        ControlMessage::RegisterAck { ok: true, .. } => Ok(()),
        ControlMessage::RegisterAck { ok: false, reason } => {
            anyhow::bail!(
                "register rejected: {}",
                reason.unwrap_or_else(|| "unknown".to_string())
            )
        }
        _ => anyhow::bail!("unexpected register ack message"),
    }
}

async fn handle_text_message(
    text: String,
    inflight: &mut Inflight,
    ws_sessions: WsSessionMap,
    relay_tx: &mpsc::UnboundedSender<Message>,
    client: &reqwest::Client,
    local_base: String,
) {
    let parsed = match serde_json::from_str::<ControlMessage>(&text) {
        Ok(msg) => msg,
        Err(err) => {
            warn!(error = %err, "invalid control message from relay");
            return;
        }
    };

    match parsed {
        ControlMessage::HttpRequestStart {
            request_id,
            method,
            path_and_query,
            headers,
        } => {
            let (body_tx, body_rx) = tokio::sync::mpsc::channel(128);
            inflight.insert(request_id, body_tx);

            let relay_tx = relay_tx.clone();
            let client = client.clone();
            tokio::spawn(async move {
                if let Err(err) = proxy_one_request(
                    request_id,
                    method,
                    path_and_query,
                    headers,
                    body_rx,
                    &client,
                    local_base,
                    &relay_tx,
                )
                .await
                {
                    error!(error = %err, %request_id, "local proxy failed");
                    let _ = send_control(
                        &relay_tx,
                        &ControlMessage::Error {
                            request_id: Some(request_id),
                            code: "local_proxy_error".to_string(),
                            message: err.to_string(),
                        },
                    );
                }
            });
        }
        ControlMessage::HttpRequestEnd { request_id } => {
            inflight.remove(&request_id);
        }
        ControlMessage::WsConnect {
            stream_id,
            path_and_query,
            headers,
            subprotocols,
        } => {
            let relay_tx = relay_tx.clone();
            tokio::spawn(async move {
                handle_ws_connect(
                    stream_id,
                    path_and_query,
                    headers,
                    subprotocols,
                    local_base,
                    relay_tx,
                    ws_sessions,
                )
                .await;
            });
        }
        ControlMessage::WsClose {
            stream_id,
            code,
            reason,
        } => {
            if let Some(sender) = ws_sessions.lock().await.get(&stream_id).cloned() {
                let _ = sender.send(WsInboundEvent::Close { code, reason });
            }
        }
        ControlMessage::Ping { ts_ms } => {
            let _ = send_control(relay_tx, &ControlMessage::Pong { ts_ms });
        }
        ControlMessage::Error { request_id, .. } => {
            if let Some(request_id) = request_id {
                inflight.remove(&request_id);
            }
        }
        ControlMessage::Pong { .. } => {}
        other => {
            warn!(?other, "unsupported control message from relay");
        }
    }
}

async fn handle_binary_message(bytes: Bytes, inflight: &mut Inflight, ws_sessions: WsSessionMap) {
    let (header, payload) = match decode_chunk_header(&bytes) {
        Ok(decoded) => decoded,
        Err(err) => {
            warn!(error = %err, "invalid binary frame from relay");
            return;
        }
    };

    match header.kind {
        StreamKind::RequestBody => {
            if let Some(sender) = inflight.get(&header.request_id) {
                if sender
                    .send(Ok(Bytes::copy_from_slice(payload)))
                    .await
                    .is_err()
                {
                    inflight.remove(&header.request_id);
                }
            }

            if header.fin {
                inflight.remove(&header.request_id);
            }
        }
        StreamKind::WsClientFrame => {
            let (opcode, data) = match decode_ws_payload(payload) {
                Ok(decoded) => decoded,
                Err(err) => {
                    warn!(error = %err, stream_id = %header.request_id, "invalid ws frame payload from relay");
                    return;
                }
            };

            if let Some(sender) = ws_sessions.lock().await.get(&header.request_id).cloned() {
                let _ = sender.send(WsInboundEvent::Frame {
                    opcode,
                    payload: Bytes::copy_from_slice(data),
                });
            }
        }
        StreamKind::ResponseBody => {
            warn!(request_id = %header.request_id, "agent received unexpected response_body chunk");
        }
        StreamKind::WsLocalFrame => {
            warn!(stream_id = %header.request_id, "agent received unexpected ws_local_frame");
        }
    }
}

async fn handle_ws_connect(
    stream_id: Uuid,
    path_and_query: String,
    headers: Vec<(String, String)>,
    subprotocols: Vec<String>,
    local_base: String,
    relay_tx: mpsc::UnboundedSender<Message>,
    ws_sessions: WsSessionMap,
) {
    let result = run_one_ws_stream(
        stream_id,
        path_and_query,
        headers,
        subprotocols,
        local_base,
        relay_tx.clone(),
        ws_sessions.clone(),
    )
    .await;

    if let Err(err) = result {
        let _ = send_control(
            &relay_tx,
            &ControlMessage::WsConnectAck {
                stream_id,
                ok: false,
                selected_subprotocol: None,
                reason: Some(err.to_string()),
            },
        );
        let mut sessions = ws_sessions.lock().await;
        sessions.remove(&stream_id);
    }
}

async fn run_one_ws_stream(
    stream_id: Uuid,
    path_and_query: String,
    headers: Vec<(String, String)>,
    subprotocols: Vec<String>,
    local_base: String,
    relay_tx: mpsc::UnboundedSender<Message>,
    ws_sessions: WsSessionMap,
) -> anyhow::Result<()> {
    let local_url = compose_local_ws_url(&local_base, &path_and_query)?;
    let mut request = local_url.as_str().into_client_request()?;

    {
        let req_headers = request.headers_mut();
        for (key, value) in headers {
            if should_skip_ws_forward_header(&key) {
                continue;
            }
            let Ok(name) = HeaderName::from_bytes(key.as_bytes()) else {
                continue;
            };
            let Ok(value) = HeaderValue::from_str(&value) else {
                continue;
            };
            req_headers.append(name, value);
        }
        if !subprotocols.is_empty() {
            let joined = subprotocols.join(", ");
            if let Ok(value) = HeaderValue::from_str(&joined) {
                req_headers.insert("sec-websocket-protocol", value);
            }
        }
    }

    let (local_ws, response) = connect_async(request).await?;
    let selected_subprotocol = response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    send_control(
        &relay_tx,
        &ControlMessage::WsConnectAck {
            stream_id,
            ok: true,
            selected_subprotocol,
            reason: None,
        },
    )?;

    let (local_sink, mut local_stream) = local_ws.split();
    let (in_tx, mut in_rx) = mpsc::unbounded_channel::<WsInboundEvent>();
    ws_sessions.lock().await.insert(stream_id, in_tx);
    let mut local_sink = local_sink;

    loop {
        tokio::select! {
            inbound = in_rx.recv() => {
                let Some(inbound) = inbound else {
                    break;
                };
                match inbound {
                    WsInboundEvent::Frame { opcode, payload } => {
                        let out = match opcode {
                            WsOpcode::Text => {
                                match std::str::from_utf8(&payload) {
                                    Ok(text) => Message::Text(text.to_string()),
                                    Err(_) => Message::Close(None),
                                }
                            }
                            WsOpcode::Binary => Message::Binary(payload.into()),
                            WsOpcode::Ping => Message::Ping(payload.into()),
                            WsOpcode::Pong => Message::Pong(payload.into()),
                            WsOpcode::Close => Message::Close(None),
                        };
                        if local_sink.send(out).await.is_err() {
                            break;
                        }
                    }
                    WsInboundEvent::Close { code, reason } => {
                        let frame = code.map(|code| tokio_tungstenite::tungstenite::protocol::CloseFrame {
                            code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(code),
                            reason: reason.unwrap_or_default().into(),
                        });
                        let _ = local_sink.send(Message::Close(frame)).await;
                        break;
                    }
                }
            }
            from_local = local_stream.next() => {
                let Some(from_local) = from_local else {
                    let _ = send_control(&relay_tx, &ControlMessage::WsClose {
                        stream_id,
                        code: Some(1000),
                        reason: Some("local ws ended".to_string()),
                    });
                    break;
                };
                match from_local {
                    Ok(Message::Text(text)) => {
                        send_ws_frame_to_relay(&relay_tx, stream_id, StreamKind::WsLocalFrame, WsOpcode::Text, text.as_bytes())?;
                    }
                    Ok(Message::Binary(bytes)) => {
                        send_ws_frame_to_relay(&relay_tx, stream_id, StreamKind::WsLocalFrame, WsOpcode::Binary, &bytes)?;
                    }
                    Ok(Message::Ping(bytes)) => {
                        send_ws_frame_to_relay(&relay_tx, stream_id, StreamKind::WsLocalFrame, WsOpcode::Ping, &bytes)?;
                    }
                    Ok(Message::Pong(bytes)) => {
                        send_ws_frame_to_relay(&relay_tx, stream_id, StreamKind::WsLocalFrame, WsOpcode::Pong, &bytes)?;
                    }
                    Ok(Message::Close(close)) => {
                        let (code, reason) = close
                            .map(|c| (Some(u16::from(c.code)), Some(c.reason.to_string())))
                            .unwrap_or((Some(1000), None));
                        let _ = send_control(&relay_tx, &ControlMessage::WsClose {
                            stream_id,
                            code,
                            reason,
                        });
                        break;
                    }
                    Ok(Message::Frame(_)) => {}
                    Err(err) => {
                        let _ = send_control(&relay_tx, &ControlMessage::WsClose {
                            stream_id,
                            code: Some(1011),
                            reason: Some(format!("local ws read error: {err}")),
                        });
                        break;
                    }
                }
            }
        }
    }

    ws_sessions.lock().await.remove(&stream_id);
    Ok(())
}

fn send_ws_frame_to_relay(
    relay_tx: &mpsc::UnboundedSender<Message>,
    stream_id: Uuid,
    kind: StreamKind,
    opcode: WsOpcode,
    payload: &[u8],
) -> anyhow::Result<()> {
    let frame = encode_chunk_frame(
        ChunkHeader {
            kind,
            request_id: stream_id,
            seq: 0,
            fin: true,
        },
        &encode_ws_payload(opcode, payload),
    );
    relay_tx
        .send(Message::Binary(frame))
        .map_err(|_| anyhow::anyhow!("relay channel closed"))
}

async fn proxy_one_request(
    request_id: Uuid,
    method: String,
    path_and_query: String,
    headers: Vec<(String, String)>,
    body_rx: BodyReceiver,
    client: &reqwest::Client,
    local_base: String,
    relay_tx: &mpsc::UnboundedSender<Message>,
) -> anyhow::Result<()> {
    let local_url = compose_local_url(&local_base, &path_and_query)?;
    let method = reqwest::Method::from_bytes(method.as_bytes())?;

    let stream_body = ReceiverStream::new(body_rx);
    let req = client
        .request(method, local_url)
        .body(reqwest::Body::wrap_stream(stream_body));
    let req = apply_forward_headers(req, &headers);

    let response = req.send().await?;

    let start = ControlMessage::HttpResponseStart {
        request_id,
        status: response.status().as_u16(),
        headers: flatten_response_headers(response.headers()),
    };
    send_control(relay_tx, &start)?;

    let mut seq = 0_u32;
    let mut byte_stream = response.bytes_stream();
    while let Some(chunk) = byte_stream.next().await {
        let chunk = chunk?;
        if chunk.is_empty() {
            continue;
        }

        let frame = encode_chunk_frame(
            ChunkHeader {
                kind: StreamKind::ResponseBody,
                request_id,
                seq,
                fin: false,
            },
            &chunk,
        );
        seq = seq.wrapping_add(1);
        relay_tx
            .send(Message::Binary(frame))
            .map_err(|_| anyhow::anyhow!("relay channel closed"))?;
    }

    send_control(relay_tx, &ControlMessage::HttpResponseEnd { request_id })?;
    Ok(())
}

fn send_control(
    relay_tx: &mpsc::UnboundedSender<Message>,
    msg: &ControlMessage,
) -> anyhow::Result<()> {
    let payload = serde_json::to_string(msg)?;
    relay_tx
        .send(Message::Text(payload))
        .map_err(|_| anyhow::anyhow!("relay channel closed"))
}

fn now_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => dur.as_millis() as u64,
        Err(_) => 0,
    }
}

fn backoff_with_jitter(attempt: u32, max_backoff_secs: u64) -> Duration {
    let exp = 2_u64.saturating_pow(attempt.min(6));
    let base_secs = exp.min(max_backoff_secs).max(1);
    let jitter_ms: u64 = rand::thread_rng().gen_range(0..500);
    Duration::from_secs(base_secs) + Duration::from_millis(jitter_ms)
}

fn derive_public_tunnel_url(relay_url: &str, tunnel_id: &str) -> Option<String> {
    let mut url = Url::parse(relay_url).ok()?;
    let scheme = match url.scheme() {
        "ws" => "http",
        "wss" => "https",
        "http" => "http",
        "https" => "https",
        _ => return None,
    };
    url.set_scheme(scheme).ok()?;

    let relay_path = url.path().trim_end_matches('/');
    let base_path = relay_path.strip_suffix("/ws").unwrap_or("");
    let tunnel_path = if base_path.is_empty() {
        format!("/t/{tunnel_id}/")
    } else {
        format!("{base_path}/t/{tunnel_id}/")
    };

    url.set_path(&tunnel_path);
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::{backoff_with_jitter, derive_public_tunnel_url};

    #[test]
    fn backoff_is_bounded() {
        let d = backoff_with_jitter(99, 30);
        assert!(d.as_secs() <= 30);
    }

    #[test]
    fn derive_public_url_from_ws() {
        let url = derive_public_tunnel_url("ws://127.0.0.1:8080/ws", "demo").expect("url");
        assert_eq!(url, "http://127.0.0.1:8080/t/demo/");
    }

    #[test]
    fn derive_public_url_with_prefix_path() {
        let url =
            derive_public_tunnel_url("wss://relay.example.com/proxy/ws", "demo").expect("url");
        assert_eq!(url, "https://relay.example.com/proxy/t/demo/");
    }
}
