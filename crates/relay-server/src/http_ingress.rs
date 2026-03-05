use std::{
    collections::VecDeque,
    time::Duration,
};

use async_stream::stream;
use axum::{
    body::{Body, Bytes},
    extract::{OriginalUri, Path, State},
    http::{header::HeaderName, HeaderMap, HeaderValue, Request, Response, StatusCode},
};
use futures_util::StreamExt;
use protocol::{encode_chunk_frame, ChunkHeader, ControlMessage, StreamKind};
use tokio::{
    sync::mpsc,
    time::{timeout, Instant},
};
use tracing::{error, warn};
use uuid::Uuid;

use crate::state::{AppState, RelayEvent};

pub async fn ingress_root(
    State(state): State<AppState>,
    Path(tunnel_id): Path<String>,
    OriginalUri(uri): OriginalUri,
    request: Request<Body>,
) -> Response<Body> {
    handle_ingress(state, tunnel_id, String::new(), uri.path_and_query().map(|pq| pq.as_str()), request)
        .await
}

pub async fn ingress_path(
    State(state): State<AppState>,
    Path((tunnel_id, path)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    request: Request<Body>,
) -> Response<Body> {
    handle_ingress(
        state,
        tunnel_id,
        path,
        uri.path_and_query().map(|pq| pq.as_str()),
        request,
    )
    .await
}

async fn handle_ingress(
    state: AppState,
    tunnel_id: String,
    tail_path: String,
    path_and_query: Option<&str>,
    request: Request<Body>,
) -> Response<Body> {
    let Some(agent) = state.get_agent(&tunnel_id).await else {
        return simple_response(StatusCode::BAD_GATEWAY, "no connected agent for tunnel");
    };

    let request_id = Uuid::new_v4();
    let target_path = extract_target_path(path_and_query, &tunnel_id, &tail_path);

    let (parts, body) = request.into_parts();
    let headers = flatten_headers(&parts.headers);

    let start = ControlMessage::HttpRequestStart {
        request_id,
        method: parts.method.to_string(),
        path_and_query: target_path,
        headers,
    };

    if send_control(&agent.sender, &start).is_err() {
        return simple_response(StatusCode::BAD_GATEWAY, "failed to deliver request start to agent");
    }

    let (tx, mut rx) = mpsc::channel(128);
    state.add_inflight(request_id, tx);

    if let Err(e) = stream_request_body(&agent.sender, request_id, body).await {
        state.remove_inflight(request_id);
        warn!(error = %e, %request_id, "request body relay failed");
        return simple_response(StatusCode::BAD_GATEWAY, "failed to stream request body");
    }

    if send_control(&agent.sender, &ControlMessage::HttpRequestEnd { request_id }).is_err() {
        state.remove_inflight(request_id);
        return simple_response(StatusCode::BAD_GATEWAY, "failed to deliver request end to agent");
    }

    let deadline = Duration::from_secs(state.request_timeout_secs);
    let started_at = Instant::now();
    let mut early_chunks = VecDeque::new();

    let (status, response_headers) = loop {
        let remaining = deadline.saturating_sub(started_at.elapsed());
        if remaining.is_zero() {
            state.remove_inflight(request_id);
            return simple_response(StatusCode::GATEWAY_TIMEOUT, "agent response timeout");
        }

        let evt = match timeout(remaining, rx.recv()).await {
            Ok(Some(event)) => event,
            Ok(None) => {
                state.remove_inflight(request_id);
                return simple_response(StatusCode::BAD_GATEWAY, "agent closed response stream");
            }
            Err(_) => {
                state.remove_inflight(request_id);
                return simple_response(StatusCode::GATEWAY_TIMEOUT, "agent response timeout");
            }
        };

        match evt {
            RelayEvent::Start { status, headers } => break (status, headers),
            RelayEvent::Body(chunk) => early_chunks.push_back(chunk),
            RelayEvent::End => {
                state.remove_inflight(request_id);
                return simple_response(StatusCode::BAD_GATEWAY, "agent response ended before start");
            }
            RelayEvent::Error { code, message } => {
                warn!(%request_id, %code, %message, "agent returned request error");
                state.remove_inflight(request_id);
                return simple_response(StatusCode::BAD_GATEWAY, "agent request error");
            }
        }
    };

    let mut builder = Response::builder().status(
        StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
    );

    if let Some(headers_map) = builder.headers_mut() {
        apply_forwarded_headers(headers_map, &response_headers);
    }

    let state_clone = state.clone();
    let body_stream = stream! {
        while let Some(chunk) = early_chunks.pop_front() {
            yield Ok::<Bytes, std::io::Error>(chunk);
        }

        while let Some(event) = rx.recv().await {
            match event {
                RelayEvent::Body(chunk) => yield Ok::<Bytes, std::io::Error>(chunk),
                RelayEvent::End => break,
                RelayEvent::Error { code, message } => {
                    error!(%request_id, %code, %message, "agent stream error");
                    break;
                }
                RelayEvent::Start { .. } => {}
            }
        }
        state_clone.remove_inflight(request_id);
    };

    builder
        .body(Body::from_stream(body_stream))
        .unwrap_or_else(|_| simple_response(StatusCode::BAD_GATEWAY, "failed to build response"))
}

async fn stream_request_body(
    sender: &tokio::sync::mpsc::UnboundedSender<axum::extract::ws::Message>,
    request_id: Uuid,
    body: Body,
) -> anyhow::Result<()> {
    let mut seq = 0_u32;
    let mut stream = body.into_data_stream();

    while let Some(next) = stream.next().await {
        let chunk = next?;
        if chunk.is_empty() {
            continue;
        }

        let frame = encode_chunk_frame(
            ChunkHeader {
                kind: StreamKind::RequestBody,
                request_id,
                seq,
                fin: false,
            },
            &chunk,
        );
        seq = seq.wrapping_add(1);

        sender
            .send(axum::extract::ws::Message::Binary(frame.into()))
            .map_err(|_| anyhow::anyhow!("agent websocket channel closed"))?;
    }

    Ok(())
}

fn send_control(
    sender: &tokio::sync::mpsc::UnboundedSender<axum::extract::ws::Message>,
    msg: &ControlMessage,
) -> anyhow::Result<()> {
    let payload = serde_json::to_string(msg)?;
    sender
        .send(axum::extract::ws::Message::Text(payload))
        .map_err(|_| anyhow::anyhow!("agent websocket channel closed"))
}

fn flatten_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (name, value) in headers {
        if is_hop_header(name.as_str()) || name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        if let Ok(value) = value.to_str() {
            out.push((name.as_str().to_string(), value.to_string()));
        }
    }
    out
}

fn apply_forwarded_headers(headers: &mut HeaderMap, forwarded: &[(String, String)]) {
    for (key, value) in forwarded {
        if is_hop_header(key) {
            continue;
        }
        let Ok(name) = HeaderName::from_bytes(key.as_bytes()) else {
            continue;
        };
        let Ok(value) = HeaderValue::from_str(value) else {
            continue;
        };
        headers.append(name, value);
    }
}

fn is_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn extract_target_path(path_and_query: Option<&str>, tunnel_id: &str, tail_path: &str) -> String {
    if let Some(pq) = path_and_query {
        let base = format!("/t/{tunnel_id}");
        let trimmed = pq.strip_prefix(&base).unwrap_or(pq);
        if trimmed.is_empty() {
            return "/".to_string();
        }
        return trimmed.to_string();
    }

    if tail_path.is_empty() {
        "/".to_string()
    } else {
        format!("/{tail_path}")
    }
}

fn simple_response(status: StatusCode, body: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::from(body.to_string()))
        .unwrap_or_else(|_| Response::new(Body::from("internal error")))
}

#[cfg(test)]
mod tests {
    use super::extract_target_path;

    #[test]
    fn path_from_tunnel_route() {
        let path = extract_target_path(Some("/t/demo/api/v1?x=1"), "demo", "api/v1");
        assert_eq!(path, "/api/v1?x=1");
    }

    #[test]
    fn path_root() {
        let path = extract_target_path(Some("/t/demo"), "demo", "");
        assert_eq!(path, "/");
    }
}
