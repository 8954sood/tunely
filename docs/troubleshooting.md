# 트러블슈팅

## `auth_tokens must not be empty`

원인:

- relay 설정에 `auth_tokens`가 없음

해결:

1. `/etc/tunely/relay.yaml` 파일을 수정
2. 토큰을 최소 1개 추가:

```yaml
auth_tokens:
  - "xxx"
```

## `404 Not Found`

원인:

- 요청 경로가 터널 라우트 형식이 아님

해결:

1. `http://<relay>/t/<tunnel_id>/...` 형식으로 요청
2. `tunnel_id`가 agent `--tunnel-id`와 일치하는지 확인

## `502 Bad Gateway`

원인:

- 요청한 `tunnel_id`에 연결된 agent가 없음

해결:

1. agent 프로세스가 실행 중인지 확인
2. agent 로그에 `agent registered`가 있는지 확인
3. relay/agent의 token, tunnel 설정이 일치하는지 확인

## `register rejected: invalid token`

원인:

- agent token이 relay `auth_tokens` 목록에 없음

해결:

1. agent `--token`과 relay `auth_tokens` 비교
2. 설정 변경 후 relay/agent 재시작

## `register rejected: tunnel_id already in use`

원인:

- 같은 `tunnel_id`를 다른 agent가 이미 사용 중

해결:

1. 기존 agent 세션 종료
2. 동일 id로 재연결하거나 다른 id 사용

## `Address already in use`

원인:

- relay listen 포트를 다른 프로세스가 사용 중

해결:

1. 해당 포트를 사용하는 프로세스 종료
2. 또는 relay listen 포트 변경

## `Connection reset without closing handshake` (agent side)

주요 원인:

- 등록 실패 후(주로 invalid token) relay가 WebSocket을 종료

해결:

1. relay 로그 먼저 확인
2. token/tunnel 설정 불일치 수정
3. relay 정상 상태에서 재시도

## Docker/WSL에서 systemd 오류

증상:

- `System has not been booted with systemd as init system`

해결:

1. `--no-systemd` 옵션으로 설치
2. 수동 실행:

```bash
tunely relay --config /etc/tunely/relay.yaml
tunely agent --relay ws://<relay>/ws --tunnel-id <id> --token <token> --local http://127.0.0.1:3000
```

## Caddy 관련 이슈

### 도메인 사용 시 agent 연결 실패

확인:

- Caddy가 `/ws*`를 relay로 프록시하는지
- agent가 `wss://<domain>/ws`로 접속하는지

### Caddy 경유 시 클라이언트 404/502

확인:

- Caddy가 `/t/*`를 relay로 프록시하는지
- DNS가 Caddy 서버를 가리키는지
