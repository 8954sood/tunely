# 로컬 개발 및 실행 가이드

기여자가 소스에서 직접 빌드하고 로컬에서 relay + agent를 실행하는 방법을 설명합니다.

## 사전 요구사항

- Rust toolchain (`rustup` 권장)
- 로컬에서 프록시할 서비스 (예: `http://127.0.0.1:3000`)

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

## 4) 동작 확인

터미널 3개를 열어 아래 순서로 실행합니다.

**터미널 1 - Relay:**

```bash
cargo run -p relay-server -- --listen 0.0.0.0:8080 --auth-token "test-token"
```

**터미널 2 - Agent:**

```bash
cargo run -p agent -- \
  --relay ws://127.0.0.1:8080/ws \
  --tunnel-id demo \
  --token test-token \
  --local http://127.0.0.1:3000
```

**터미널 3 - 요청 테스트:**

```bash
curl -v http://127.0.0.1:8080/t/demo/
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
