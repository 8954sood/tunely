# 로컬 개발 및 실행 가이드

기여자가 소스에서 직접 빌드하고 로컬에서 relay + agent를 실행하는 방법을 설명합니다.

## 사전 요구사항

- Rust toolchain (`rustup` 권장)
- 로컬에서 프록시할 서비스 (예: `http://127.0.0.1:3000`) — 없으면 내장 echo-server 사용 가능

## 1) 빌드

```bash
cargo build --release -p tunely -p relay-server -p agent
```

바이너리 위치:

- `target/release/tunely`
- `target/release/relay-server`
- `target/release/agent`

> 개발 중에는 `--release` 없이 `cargo build`로 빠르게 빌드할 수 있습니다.

## 2) Relay 실행

### CLI 인자로 실행 (설정 파일 없이)

```bash
cargo run -p relay-server -- --listen 0.0.0.0:8080 --auth-token "my-secret-token"
```

### 설정 파일로 실행

`relay.yaml` 파일을 만들고:

```yaml
listen: "0.0.0.0:8080"
auth_tokens:
  - "my-secret-token"
request_timeout_secs: 60
```

실행:

```bash
cargo run -p relay-server -- --config relay.yaml
```

또는 `tunely` 래퍼를 사용:

```bash
cargo run -p tunely -- relay --config relay.yaml
```

## 3) Agent 실행

```bash
cargo run -p agent -- \
  --relay ws://127.0.0.1:8080/ws \
  --tunnel-id demo \
  --token my-secret-token \
  --local http://127.0.0.1:3000
```

또는 `tunely` 래퍼를 사용:

```bash
cargo run -p tunely -- agent \
  --relay ws://127.0.0.1:8080/ws \
  --tunnel-id demo \
  --token my-secret-token \
  --local http://127.0.0.1:3000
```

### Agent 옵션

| 인자 | 기본값 | 설명 |
|------|--------|------|
| `--relay` | (필수) | Relay WebSocket URL |
| `--tunnel-id` | (필수) | 터널 식별자 |
| `--token` | (필수) | Relay auth_tokens에 포함된 토큰 |
| `--local` | (필수) | 프록시할 로컬 서비스 URL |
| `--ping-interval-secs` | `20` | WebSocket ping 간격 |
| `--max-backoff-secs` | `30` | 재연결 최대 백오프 |

## 4) 동작 확인 (echo-server + test-client)

`examples/` 에 테스트용 서버와 클라이언트가 포함되어 있습니다.

터미널 4개를 열어 아래 순서로 실행합니다.

**터미널 1 - echo-server** (로컬 서비스 역할):

```bash
cargo run -p echo-server
```

기본 포트 `3000`에서 실행됩니다. 포트 변경: `cargo run -p echo-server -- 4000`

엔드포인트:

- `GET /` — hello 응답
- `ANY /echo` — 요청 메서드 JSON 반환
- `ANY /headers` — 수신된 헤더 JSON 반환
- `ANY /body` — 요청 body echo
- `GET /ws` — WebSocket echo (text/binary 그대로 반환)

**터미널 2 - Relay:**

```bash
cargo run -p relay-server -- --listen 0.0.0.0:8080 --auth-token "test-token"
```

**터미널 3 - Agent:**

```bash
cargo run -p agent -- \
  --relay ws://127.0.0.1:8080/ws \
  --tunnel-id demo \
  --token test-token \
  --local http://127.0.0.1:3000
```

**터미널 4 - test-client** (HTTP + WebSocket 자동 테스트):

```bash
cargo run -p test-client
```

기본 대상: `http://127.0.0.1:8080/t/demo`. 변경: `cargo run -p test-client -- http://127.0.0.1:8080/t/demo`

출력 예시:

```
=== tunely test client ===
target: http://127.0.0.1:8080/t/demo

--- HTTP tests ---

[PASS] GET /                           -> 200 OK | hello from echo-server
[PASS] GET /echo                       -> 200 OK | {"method":"GET"}
[PASS] POST /echo                      -> 200 OK | {"method":"POST"}
[PASS] PUT /echo                       -> 200 OK | {"method":"PUT"}
[PASS] PATCH /echo                     -> 200 OK | {"method":"PATCH"}
[PASS] DELETE /echo                    -> 200 OK | {"method":"DELETE"}
[PASS] OPTIONS /echo                   -> 200 OK | {"method":"OPTIONS"}
[PASS] HEAD /echo                      -> 200 OK
[PASS] POST /body (with body)          -> 200 OK | body echoed
[PASS] PUT /body (with body)           -> 200 OK | body echoed
[PASS] PATCH /body (with body)         -> 200 OK | body echoed
[PASS] GET /headers (x-test)           -> 200 OK | x-test forwarded

--- WebSocket tests ---

[PASS] WS connect                      -> ws://127.0.0.1:8080/t/demo/ws
[PASS] WS text echo                    -> echo: hello tunely
[PASS] WS binary echo                  -> 4 bytes
[PASS] WS close                        -> sent

=== result: (16/16) ===
ALL TESTS PASSED
```

모두 `[PASS]`이면 HTTP/WebSocket 터널링이 정상 동작하는 것입니다.

수동으로 확인하려면:

```bash
curl -v http://127.0.0.1:8080/t/demo/
curl -v http://127.0.0.1:8080/t/demo/headers -H "x-test: tunely"
```

## 5) 이미 설치된 환경에서 개발 버전 실행

시스템에 install 스크립트로 설치된 tunely가 있다면:

- 설치 경로: `/opt/tunely/`, 설정: `/etc/tunely/`
- systemd 서비스: `tunely-relay`, `tunely-agent`

**충돌 방지:**

```bash
# 기존 서비스 중지
sudo systemctl stop tunely-relay tunely-agent

# 개발 버전은 다른 포트로 실행
cargo run -p relay-server -- --listen 0.0.0.0:9090 --auth-token "dev-token"
```

개발이 끝나면 기존 서비스를 다시 시작할 수 있습니다:

```bash
sudo systemctl start tunely-relay tunely-agent
```

## 관련 문서

- 빌드/릴리즈: [build-and-release.md](build-and-release.md)
- Ubuntu 배포: [ubuntu.md](ubuntu.md)
- 트러블슈팅: [troubleshooting.md](troubleshooting.md)
