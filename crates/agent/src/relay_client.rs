use std::{
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    decode_chunk_header, encode_chunk_frame, ChunkHeader, ControlMessage, StreamKind,
};
use rand::Rng;
use tokio::{
    sync::mpsc,
    time::{interval, timeout},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    config::Config,
    inflight::{BodyReceiver, Inflight},
    local_proxy::{apply_forward_headers, compose_local_url, flatten_response_headers},
};

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
    info!(tunnel_id = %config.tunnel_id, "agent registered");

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

    while let Some(msg) = ws_reader.next().await {
        let msg = msg?;
        match msg {
            Message::Text(text) => {
                handle_text_message(
                    text,
                    &mut inflight,
                    &out_tx,
                    &client,
                    config.local.clone(),
                )
                .await;
            }
            Message::Binary(bytes) => {
                handle_binary_message(bytes.into(), &mut inflight).await;
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
            anyhow::bail!("register rejected: {}", reason.unwrap_or_else(|| "unknown".to_string()))
        }
        _ => anyhow::bail!("unexpected register ack message"),
    }
}

async fn handle_text_message(
    text: String,
    inflight: &mut Inflight,
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

async fn handle_binary_message(bytes: Bytes, inflight: &mut Inflight) {
    let (header, payload) = match decode_chunk_header(&bytes) {
        Ok(decoded) => decoded,
        Err(err) => {
            warn!(error = %err, "invalid binary frame from relay");
            return;
        }
    };

    if header.kind != StreamKind::RequestBody {
        warn!(request_id = %header.request_id, "agent received non-request chunk");
        return;
    }

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
    let req = client.request(method, local_url).body(reqwest::Body::wrap_stream(stream_body));
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

fn send_control(relay_tx: &mpsc::UnboundedSender<Message>, msg: &ControlMessage) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::backoff_with_jitter;

    #[test]
    fn backoff_is_bounded() {
        let d = backoff_with_jitter(99, 30);
        assert!(d.as_secs() <= 30);
    }
}
