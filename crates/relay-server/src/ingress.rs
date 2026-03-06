use axum::{
    body::Body,
    extract::{FromRequestParts, OriginalUri, Path, State, ws::WebSocketUpgrade},
    http::{HeaderMap, Method, Request, Response},
};

use crate::{http_ingress, state::AppState, ws_tunnel};

pub async fn ingress_root(
    State(state): State<AppState>,
    Path(tunnel_id): Path<String>,
    OriginalUri(uri): OriginalUri,
    request: Request<Body>,
) -> Response<Body> {
    if is_ws_upgrade_candidate(request.method(), request.headers()) {
        let (mut parts, body) = request.into_parts();
        let headers = parts.headers.clone();
        let ws = maybe_extract_ws_upgrade(&mut parts).await;
        let request = Request::from_parts(parts, body);

        if let Some(ws) = ws {
            let target_path = http_ingress::extract_target_path(
                uri.path_and_query().map(|pq| pq.as_str()),
                &tunnel_id,
                "",
            );
            return ws_tunnel::upgrade_client_ws(state, tunnel_id, target_path, headers, ws).await;
        }

        return http_ingress::ingress_root(
            State(state),
            Path(tunnel_id),
            OriginalUri(uri),
            request,
        )
        .await;
    }

    http_ingress::ingress_root(State(state), Path(tunnel_id), OriginalUri(uri), request).await
}

pub async fn ingress_path(
    State(state): State<AppState>,
    Path((tunnel_id, path)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    request: Request<Body>,
) -> Response<Body> {
    if is_ws_upgrade_candidate(request.method(), request.headers()) {
        let (mut parts, body) = request.into_parts();
        let headers = parts.headers.clone();
        let ws = maybe_extract_ws_upgrade(&mut parts).await;
        let request = Request::from_parts(parts, body);

        if let Some(ws) = ws {
            let target_path = http_ingress::extract_target_path(
                uri.path_and_query().map(|pq| pq.as_str()),
                &tunnel_id,
                &path,
            );
            return ws_tunnel::upgrade_client_ws(state, tunnel_id, target_path, headers, ws).await;
        }

        return http_ingress::ingress_path(
            State(state),
            Path((tunnel_id, path)),
            OriginalUri(uri),
            request,
        )
        .await;
    }

    http_ingress::ingress_path(
        State(state),
        Path((tunnel_id, path)),
        OriginalUri(uri),
        request,
    )
    .await
}

async fn maybe_extract_ws_upgrade(
    parts: &mut axum::http::request::Parts,
) -> Option<WebSocketUpgrade> {
    if parts.method != Method::GET {
        return None;
    }
    WebSocketUpgrade::from_request_parts(parts, &()).await.ok()
}

fn is_ws_upgrade_candidate(method: &Method, headers: &HeaderMap) -> bool {
    if method != Method::GET {
        return false;
    }
    has_csv_token(headers, "connection", "upgrade")
        && header_eq(headers, "upgrade", "websocket")
        && headers.contains_key("sec-websocket-key")
        && has_csv_token(headers, "sec-websocket-version", "13")
}

fn has_csv_token(headers: &HeaderMap, name: &str, expected: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case(expected))
        })
}

fn header_eq(headers: &HeaderMap, name: &str, expected: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, Method};

    use super::is_ws_upgrade_candidate;

    #[test]
    fn ws_upgrade_candidate_true_for_valid_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "connection",
            HeaderValue::from_static("keep-alive, Upgrade"),
        );
        headers.insert("upgrade", HeaderValue::from_static("websocket"));
        headers.insert("sec-websocket-key", HeaderValue::from_static("x"));
        headers.insert("sec-websocket-version", HeaderValue::from_static("13"));

        assert!(is_ws_upgrade_candidate(&Method::GET, &headers));
    }

    #[test]
    fn ws_upgrade_candidate_false_for_normal_get() {
        let headers = HeaderMap::new();
        assert!(!is_ws_upgrade_candidate(&Method::GET, &headers));
    }
}
