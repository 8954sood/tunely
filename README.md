# Tunely

Rust로 구현한 경량 reverse tunnel MVP입니다.

- 로컬 `agent`가 먼저 `relay-server`에 outbound WebSocket 연결
- 외부 HTTP 요청을 `relay-server`가 `tunnel_id` 기준으로 agent에 전달
- agent가 `localhost`로 프록시하고 응답을 relay를 통해 반환
- 바이너리 바디(이미지/파일) 포함 전달 가능

## 구성

워크스페이스는 3개 크레이트로 구성됩니다.

- `crates/protocol`: relay-agent 공통 메시지/프레임 타입
- `crates/relay-server`: 공인 진입점(HTTP + WebSocket)
- `crates/agent`: 로컬 포워딩 클라이언트

## 핵심 동작

1. Agent가 `/ws`로 접속 후 `RegisterAgent { tunnel_id, token }` 전송
2. Relay가 토큰 검증 후 세션 등록
3. 외부 요청 `http://relay/t/<tunnel_id>/...` 수신
4. Relay가 `request_id` 생성 후 `HttpRequestStart` + request body chunk 전송
5. Agent가 로컬 서버로 요청 전달 (`reqwest`)
6. Agent가 `HttpResponseStart` + response body chunk + `HttpResponseEnd` 전송
7. Relay가 외부 클라이언트로 스트리밍 응답

## 프로토콜

### Control message (JSON text frame)

- `register_agent`
- `register_ack`
- `http_request_start`
- `http_request_end`
- `http_response_start`
- `http_response_end`
- `error`
- `ping` / `pong`

정의: `crates/protocol/src/message.rs`

### Body chunk (WebSocket binary frame)

바디는 JSON이 아닌 raw bytes로 전송합니다.

헤더 포맷:

- `version: u8`
- `stream_kind: u8` (`0=request_body`, `1=response_body`)
- `request_id: [u8;16]` (UUID)
- `seq: u32 (BE)`
- `flags: u8` (`bit0=fin`)
- `payload: [u8]`

정의: `crates/protocol/src/frame.rs`

## 실행 가이드

운영체제별 실행 문서는 `docs/`로 분리되어 있습니다.

- Ubuntu(amd64/arm64): [docs/ubuntu.md](docs/ubuntu.md)
- Windows(x64): [docs/windows.md](docs/windows.md)

## 테스트

```bash
cargo test --workspace
```

## 단일 파일 배포 (Ubuntu amd64/arm64, Windows x64)

Rust 바이너리라서 설치형 패키지 없이 실행 파일 1개로 실행할 수 있습니다.

- `relay-server` 실행 파일 1개
- `agent` 실행 파일 1개

즉, 역할별로는 1파일 실행이 가능합니다(프로젝트 전체는 2개 바이너리).

### GitHub Actions로 자동 빌드

워크플로: `.github/workflows/release-binaries.yml`

- 태그 푸시(`v*`) 또는 수동 실행 시 아래 아티팩트 생성
- `tunely-linux-amd64.tar.gz`
  - `relay-server` (x86_64-unknown-linux-musl, 정적 링크)
  - `agent` (x86_64-unknown-linux-musl, 정적 링크)
  - `install-relay.sh`, `install-agent.sh`, `uninstall-tunely.sh`
- `tunely-linux-arm64.tar.gz`
  - `relay-server` (aarch64-unknown-linux-musl, 정적 링크)
  - `agent` (aarch64-unknown-linux-musl, 정적 링크)
  - `install-relay.sh`, `install-agent.sh`, `uninstall-tunely.sh`
- `tunely-windows-x64.zip`
  - `relay-server.exe`
  - `agent.exe`

### 로컬에서 직접 빌드

#### Ubuntu amd64/arm64 머신에서 직접 빌드

```bash
cargo build --release -p relay-server -p agent
```

결과:

- `target/release/relay-server`
- `target/release/agent`

#### Linux/macOS에서 Ubuntu용 크로스 빌드 (amd64)

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target x86_64-unknown-linux-musl -p relay-server -p agent
```

결과:

- `target/x86_64-unknown-linux-musl/release/relay-server`
- `target/x86_64-unknown-linux-musl/release/agent`

#### Linux/macOS에서 Ubuntu용 크로스 빌드 (arm64)

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target aarch64-unknown-linux-musl -p relay-server -p agent
```

결과:

- `target/aarch64-unknown-linux-musl/release/relay-server`
- `target/aarch64-unknown-linux-musl/release/agent`

#### Windows에서 빌드

```powershell
cargo build --release --target x86_64-pc-windows-msvc -p relay-server -p agent
```

결과:

- `target/x86_64-pc-windows-msvc/release/relay-server.exe`
- `target/x86_64-pc-windows-msvc/release/agent.exe`

## 트러블슈팅

### 1) `404 Not Found`가 나오는 경우

- 요청 경로가 `http://<relay>/t/<tunnel_id>/...` 형식인지 확인
- relay/agent를 **최신 코드로 재시작**했는지 확인

### 2) `502 Bad Gateway`가 나오는 경우

- 해당 `tunnel_id`로 연결된 agent가 없는 상태
- agent 로그에서 `agent registered` 확인

### 3) `register rejected: invalid tunnel_id/token`

- relay의 `--auth`와 agent의 `--tunnel-id`, `--token` 값이 일치해야 함

### 4) `Address already in use`

- 포트 충돌입니다. 기존 프로세스를 종료하거나 다른 포트를 사용하세요.

## 현재 MVP 범위

- HTTP reverse proxy tunnel
- 단일 relay + 복수 tunnel 구조
- tunnel-id 라우팅
- shared secret 토큰 인증
- 연결/요청/에러 기본 로깅

## 향후 확장 포인트

- WebSocket passthrough
- HTTPS/WSS (TLS 종료)
- 서브도메인 라우팅
- TCP tunnel
- 다중 relay 확장 및 영속 세션 관리
