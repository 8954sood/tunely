use axum::{
    Json, Router,
    body::Bytes,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::{HeaderMap, Method},
    response::IntoResponse,
    routing::{any, get},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};

#[tokio::main]
async fn main() {
    let port = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "3000".to_string());
    let addr = format!("0.0.0.0:{port}");

    let app = Router::new()
        .route("/", get(hello))
        .route("/echo", any(echo_method))
        .route("/headers", any(show_headers))
        .route("/body", any(echo_body))
        .route("/ws", get(ws_handler));

    println!("[echo-server] listening on {addr}");
    println!("  GET  /        -> hello");
    println!("  ANY  /echo    -> method echo (returns method name)");
    println!("  ANY  /headers -> request headers");
    println!("  ANY  /body    -> body echo (returns request body)");
    println!("  GET  /ws      -> websocket echo");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn hello() -> &'static str {
    "hello from echo-server\n"
}

async fn echo_method(method: Method) -> Json<Value> {
    Json(json!({ "method": method.as_str() }))
}

async fn show_headers(method: Method, headers: HeaderMap) -> Json<Value> {
    let mut map = serde_json::Map::new();
    for (name, value) in &headers {
        map.insert(
            name.to_string(),
            Value::String(value.to_str().unwrap_or("?").to_string()),
        );
    }
    Json(json!({ "method": method.as_str(), "headers": map }))
}

async fn echo_body(method: Method, body: Bytes) -> Json<Value> {
    let body_str = String::from_utf8_lossy(&body).to_string();
    Json(json!({ "method": method.as_str(), "body": body_str, "length": body.len() }))
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(socket: WebSocket) {
    let (mut tx, mut rx) = socket.split();
    println!("[ws] client connected");

    while let Some(msg) = rx.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                println!("[ws] read error: {e}");
                break;
            }
        };

        match &msg {
            Message::Text(text) => {
                println!("[ws] text: {text}");
                let reply = format!("echo: {text}");
                if tx.send(Message::Text(reply.into())).await.is_err() {
                    break;
                }
            }
            Message::Binary(bytes) => {
                println!("[ws] binary: {} bytes", bytes.len());
                if tx.send(Message::Binary(bytes.clone())).await.is_err() {
                    break;
                }
            }
            Message::Ping(bytes) => {
                if tx.send(Message::Pong(bytes.clone())).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => {
                println!("[ws] client closed");
                break;
            }
            _ => {}
        }
    }

    println!("[ws] client disconnected");
}
