# Tunely

Tunely는 Rust로 구현한 경량 reverse tunnel MVP입니다.

- Agent가 relay에 outbound WebSocket 연결을 맺고 유지
- Relay가 외부 HTTP 요청을 `tunnel_id` 기준으로 agent에 전달
- Agent가 로컬 서버(`localhost`)로 프록시 후 응답 반환
- WebSocket 요청도 `/t/<tunnel_id>/...` 경로로 터널링 지원
- 바이너리 바디(이미지/파일) 전달 지원
- (선택) Cloudflare + Caddy Admin API 기반 동적 서브도메인 자동 생성/삭제 지원

## 빠른 시작

```bash
tunely help
tunely relay --config /etc/tunely/relay.yaml
tunely agent --relay ws://127.0.0.1:8080/ws --tunnel-id demo --token xxx --local http://127.0.0.1:3000
curl -v http://127.0.0.1:8080/t/demo/
curl -s http://127.0.0.1:8080/healthz
curl -s http://127.0.0.1:8080/readyz
```

Relay 설정 파일:

- 경로: `/etc/tunely/relay.yaml`
- 파일이 없으면 설치 스크립트가 주석 템플릿으로 생성

예시:

```yaml
listen: "0.0.0.0:8080"
auth_tokens:
  - "xxx"
  - "yyy"
request_timeout_secs: 60
```

동적 서브도메인 모드 예시:

```yaml
listen: "127.0.0.1:8080"
auth_tokens:
  - "xxx"
enable_dynamic_subdomain: true
base_domain: "example.com"
cloudflare_api_token: "cf_token"
cloudflare_zone_id: "cf_zone_id"
public_origin: "1.2.3.4"
caddy_admin_url: "http://127.0.0.1:2019"
caddy_upstream: "127.0.0.1:8080"
```

```bash
tunely agent \
  --relay wss://relay.example.com/ws \
  --tunnel-id demo \
  --token xxx \
  --local http://127.0.0.1:3000 \
  --request-subdomain
```

## 문서

- 로컬 개발/실행: [docs/local-dev.md](docs/local-dev.md)
- Ubuntu 설치/실행: [docs/ubuntu.md](docs/ubuntu.md)
- Windows 실행: [docs/windows.md](docs/windows.md)
- 설정 레퍼런스: [docs/config.md](docs/config.md)
- Caddy 설정: [docs/caddy.md](docs/caddy.md)
- 빌드/릴리즈: [docs/build-and-release.md](docs/build-and-release.md)
- 트러블슈팅: [docs/troubleshooting.md](docs/troubleshooting.md)

## 워크스페이스

- `crates/tunely`: 통합 CLI (`tunely relay|agent|help`)
- `crates/protocol`: relay-agent 공통 프로토콜 타입
- `crates/relay-server`: HTTP ingress + WebSocket relay
- `crates/agent`: 로컬 포워딩 클라이언트

## 프로토콜

- 제어 메시지(JSON text frame): `register_agent`, `http_request_start`, `http_response_start`, `ping/pong` 등
- 바디 전송(binary frame): 스트림 메타데이터를 포함한 raw payload chunk

정의 위치:

- `crates/protocol/src/message.rs`
- `crates/protocol/src/frame.rs`

## 테스트

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## MVP 범위

- HTTP reverse proxy tunnel
- WebSocket passthrough tunnel
- 단일 relay + 복수 agent
- `tunnel_id` routing
- shared secret token 인증
- 기본 로깅(connect/reconnect/request/error)
