use axum::{
    body::Body,
    extract::{FromRequestParts, OriginalUri, Path, State, ws::WebSocketUpgrade},
    http::{Method, Request, Response},
};

use crate::{http_ingress, state::AppState, ws_tunnel};

pub async fn ingress_root(
    State(state): State<AppState>,
    Path(tunnel_id): Path<String>,
    OriginalUri(uri): OriginalUri,
    request: Request<Body>,
) -> Response<Body> {
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

    http_ingress::ingress_root(State(state), Path(tunnel_id), OriginalUri(uri), request).await
}

pub async fn ingress_path(
    State(state): State<AppState>,
    Path((tunnel_id, path)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    request: Request<Body>,
) -> Response<Body> {
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
