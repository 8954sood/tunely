use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

struct TestResult {
    passed: u32,
    total: u32,
}

impl TestResult {
    fn new() -> Self {
        Self {
            passed: 0,
            total: 0,
        }
    }

    fn pass(&mut self, label: &str, detail: &str) {
        self.total += 1;
        self.passed += 1;
        println!("[PASS] {label:<30} {detail}");
    }

    fn fail(&mut self, label: &str, detail: &str) {
        self.total += 1;
        println!("[FAIL] {label:<30} {detail}");
    }
}

#[tokio::main]
async fn main() {
    let base = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:8080/t/demo".to_string());

    println!("=== tunely test client ===");
    println!("target: {base}\n");

    let mut r = TestResult::new();

    test_http(&base, &mut r).await;
    test_ws(&base, &mut r).await;

    println!("\n=== result: ({}/{}) ===", r.passed, r.total);
    if r.passed == r.total {
        println!("ALL TESTS PASSED");
    } else {
        println!("{} TEST(S) FAILED", r.total - r.passed);
        std::process::exit(1);
    }
}

async fn test_http(base: &str, r: &mut TestResult) {
    let client = reqwest::Client::new();

    println!("--- HTTP tests ---\n");

    // GET /
    test_method(&client, base, "GET", "/", r).await;

    // GET /echo
    test_method(&client, base, "GET", "/echo", r).await;

    // POST /echo
    test_method(&client, base, "POST", "/echo", r).await;

    // PUT /echo
    test_method(&client, base, "PUT", "/echo", r).await;

    // PATCH /echo
    test_method(&client, base, "PATCH", "/echo", r).await;

    // DELETE /echo
    test_method(&client, base, "DELETE", "/echo", r).await;

    // OPTIONS /echo
    test_method(&client, base, "OPTIONS", "/echo", r).await;

    // HEAD /echo
    test_method(&client, base, "HEAD", "/echo", r).await;

    // POST /body with body
    test_body(&client, base, "POST", r).await;

    // PUT /body with body
    test_body(&client, base, "PUT", r).await;

    // PATCH /body with body
    test_body(&client, base, "PATCH", r).await;

    // GET /headers with custom header
    test_headers(&client, base, r).await;
}

async fn test_method(
    client: &reqwest::Client,
    base: &str,
    method: &str,
    path: &str,
    r: &mut TestResult,
) {
    let label = format!("{method} {path}");
    let url = format!("{base}{path}");
    let http_method: reqwest::Method = method.parse().unwrap();

    match client.request(http_method, &url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if method == "HEAD" {
                // HEAD has no body, just check status
                if status.is_success() {
                    r.pass(&label, &format!("-> {status}"));
                } else {
                    r.fail(&label, &format!("-> {status}"));
                }
                return;
            }
            let text = resp.text().await.unwrap_or_default();
            if status.is_success() {
                r.pass(&label, &format!("-> {status} | {}", text.trim()));
            } else {
                r.fail(&label, &format!("-> {status} | {}", text.trim()));
            }
        }
        Err(e) => r.fail(&label, &format!("-> {e}")),
    }
}

async fn test_body(client: &reqwest::Client, base: &str, method: &str, r: &mut TestResult) {
    let label = format!("{method} /body (with body)");
    let url = format!("{base}/body");
    let http_method: reqwest::Method = method.parse().unwrap();
    let payload = "hello tunely body";

    match client.request(http_method, &url).body(payload).send().await {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let body_echoed = text.contains(payload);
            if status.is_success() && body_echoed {
                r.pass(&label, &format!("-> {status} | body echoed"));
            } else {
                r.fail(
                    &label,
                    &format!("-> {status} | body_echoed={body_echoed} | {}", text.trim()),
                );
            }
        }
        Err(e) => r.fail(&label, &format!("-> {e}")),
    }
}

async fn test_headers(client: &reqwest::Client, base: &str, r: &mut TestResult) {
    let label = "GET /headers (x-test)";
    let url = format!("{base}/headers");

    match client.get(&url).header("x-test", "tunely").send().await {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let has_header = text.contains("x-test");
            if status.is_success() && has_header {
                r.pass(label, &format!("-> {status} | x-test forwarded"));
            } else {
                r.fail(
                    label,
                    &format!("-> {status} | x-test forwarded: {has_header}"),
                );
            }
        }
        Err(e) => r.fail(label, &format!("-> {e}")),
    }
}

async fn test_ws(base: &str, r: &mut TestResult) {
    println!("\n--- WebSocket tests ---\n");

    let ws_url = http_to_ws(base);
    let ws_url = format!("{ws_url}/ws");

    let (ws, _) = match connect_async(&ws_url).await {
        Ok(conn) => {
            r.pass("WS connect", &format!("-> {ws_url}"));
            conn
        }
        Err(e) => {
            r.fail("WS connect", &format!("-> {e}"));
            return;
        }
    };

    let (mut tx, mut rx) = ws.split();

    // text echo
    tx.send(Message::Text("hello tunely".into())).await.unwrap();
    if let Some(Ok(msg)) = rx.next().await {
        let text = msg.into_text().unwrap_or_default();
        if text.contains("hello tunely") {
            r.pass("WS text echo", &format!("-> {text}"));
        } else {
            r.fail("WS text echo", &format!("-> unexpected: {text}"));
        }
    } else {
        r.fail("WS text echo", "-> no response");
    }

    // binary echo
    let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    tx.send(Message::Binary(data.clone().into())).await.unwrap();
    if let Some(Ok(msg)) = rx.next().await {
        match msg {
            Message::Binary(bytes) => {
                if bytes == data {
                    r.pass("WS binary echo", &format!("-> {} bytes", bytes.len()));
                } else {
                    r.fail("WS binary echo", "-> data mismatch");
                }
            }
            other => r.fail("WS binary echo", &format!("-> unexpected: {other:?}")),
        }
    } else {
        r.fail("WS binary echo", "-> no response");
    }

    // close
    if tx.send(Message::Close(None)).await.is_ok() {
        r.pass("WS close", "-> sent");
    } else {
        r.fail("WS close", "-> send failed");
    }
}

fn http_to_ws(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        url.to_string()
    }
}
