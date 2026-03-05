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

## 실행 방법

### 1) 로컬 앱 실행 (예시)

```bash
python3 -m http.server 3000
```

### 2) Relay 실행

```bash
cargo run -p relay-server -- --listen 127.0.0.1:8080 --auth demo=xxx
```

옵션:

- `--listen` (기본 `0.0.0.0:8080`)
- `--auth` 형식: `tunnel=token,tunnel2=token2`
- `--request-timeout-secs` (기본 `60`)

### 3) Agent 실행

```bash
cargo run -p agent -- \
  --relay ws://127.0.0.1:8080/ws \
  --tunnel-id demo \
  --token xxx \
  --local http://127.0.0.1:3000
```

옵션:

- `--ping-interval-secs` (기본 `20`)
- `--max-backoff-secs` (기본 `30`)

### 4) 외부 요청

```bash
curl -v http://127.0.0.1:8080/t/demo/
```

`/t/demo`와 `/t/demo/` 둘 다 지원합니다.

## 바이너리 전달 확인

```bash
# 로컬 3000 서버에 image.png가 있다고 가정
curl -v http://127.0.0.1:8080/t/demo/image.png --output /tmp/out.png
cmp image.png /tmp/out.png
```

## 테스트

```bash
cargo test --workspace
```

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
