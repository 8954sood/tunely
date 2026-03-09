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

## `register rejected: dynamic subdomain is not enabled on relay`

원인:

- agent가 `--request-subdomain`으로 요청했지만 relay 동적 서브도메인 설정이 비활성화됨

해결:

1. relay 설정(`relay.yaml`)에 `enable_dynamic_subdomain: true` 적용
2. `base_domain`, `cloudflare_api_token`, `cloudflare_zone_id`, `public_origin`, `caddy_admin_url` 값이 모두 설정됐는지 확인
3. relay 재시작 후 agent 재연결

## `register rejected: invalid tunnel_id for subdomain mode ...`

원인:

- `--request-subdomain` 모드에서 `tunnel_id`가 DNS 라벨 규칙을 만족하지 않음

해결:

1. `tunnel_id`를 소문자/숫자/하이픈(`-`)만 사용해 재시도
2. 언더스코어(`_`), 대문자, 공백이 포함되지 않게 수정

## `register rejected: subdomain provisioning failed: ...`

원인:

- Cloudflare DNS 생성/갱신 실패
- Caddy Admin API 라우트 생성 실패

해결:

1. `cloudflare_api_token` 권한 확인(해당 zone DNS 수정 가능 권한)
2. `cloudflare_zone_id`, `base_domain`, `public_origin` 값 확인
3. `caddy_admin_url` 접근 가능 여부 확인 (예: `curl -s http://127.0.0.1:2019/config/`)
4. Caddy가 실행 중인지 및 relay가 Caddy Admin에 접근 가능한지 확인

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

- `relay.example.com` 블록에서 `/ws*`를 relay로 프록시하는지
- agent가 `wss://relay.example.com/ws`로 접속하는지
- agent가 실수로 `wss://<domain>/t/<id>/ws`로 연결하지 않았는지

### Caddy 경유 시 클라이언트 404/502

확인:

- subdomain rewrite가 `/t/<tunnel_id>{uri}` 형태로 설정되어 있는지
- 예: `wss://demo.example.com/ws` -> relay 내부 `/t/demo/ws`
- DNS가 Caddy 서버를 가리키는지
- 동적 모드라면 relay 로그에 subdomain provisioning 성공 로그가 있는지

### `WebSocket connection to 'wss://.../t/<id>/...' failed`

확인:

- 해당 `tunnel_id` agent가 연결되어 있는지 (`agent registered` 로그)
- local 서버의 WS endpoint 경로가 실제로 존재하는지
- Caddy matcher 순서에서 `relay.example.com /ws*`와 `*.example.com rewrite`가 충돌하지 않는지

### 동적 모드에서 agent 연결 해제 후 DNS가 남아 있음

확인:

- relay 로그에 `subdomain deprovision failed`가 있는지
- Cloudflare 레코드 comment가 `managed-by=tunely`인지
- relay가 Cloudflare API에 계속 접근 가능한지
