# 설정 레퍼런스

Tunely의 주요 설정(`relay.yaml`, CLI 플래그)과 동적 서브도메인 모드 관련 옵션을 정리합니다.

## 1) Relay 설정 파일

기본 경로:

- `/etc/tunely/relay.yaml`

`tunely relay`는 이 파일을 기본값으로 사용하고, CLI 인자로 같은 키를 넘기면 CLI 값이 우선합니다.

### 1-1) 공통 키

| 키 | 타입 | 기본값 | 필수 | 설명 |
|---|---|---|---|---|
| `listen` | string | `"0.0.0.0:8080"` | 아니오 | relay bind 주소 |
| `auth_tokens` | string[] | 없음 | 예 | agent 인증 토큰 목록 (최소 1개) |
| `request_timeout_secs` | number | `60` | 아니오 | 요청 처리 타임아웃(초) |

### 1-2) 동적 서브도메인 키

아래 키는 `enable_dynamic_subdomain: true`일 때 모두 필요합니다.

| 키 | 타입 | 필수 | 설명 |
|---|---|---|---|
| `enable_dynamic_subdomain` | bool | 아니오 | 동적 서브도메인 모드 활성화 |
| `base_domain` | string | 예 | 발급 기준 도메인 (`example.com`) |
| `cloudflare_api_token` | string | 예 | Cloudflare DNS API 토큰 |
| `cloudflare_zone_id` | string | 예 | Cloudflare zone id |
| `public_origin` | string | 예 | DNS 레코드 대상(IP 또는 hostname) |
| `caddy_admin_url` | string | 예 | Caddy Admin API 주소 (`http://127.0.0.1:2019`) |
| `caddy_upstream` | string | 아니오 | Caddy reverse proxy 대상 (`127.0.0.1:8080`) |

참고:

- `caddy_upstream` 미지정 시 `listen` 값을 사용합니다.
- 동적 모드 DNS 레코드는 Cloudflare에서 `proxied=false`(DNS only)로 생성됩니다.

### 1-3) Cloudflare `zone_id` / API 토큰 발급 방법

#### `cloudflare_zone_id` 확인 (대시보드)

1. Cloudflare Dashboard 로그인
2. 사용할 도메인(zone) 선택
3. 우측 사이드바의 `API` 섹션에서 `Zone ID` 복사

#### `cloudflare_zone_id` 확인 (API)

```bash
curl -s "https://api.cloudflare.com/client/v4/zones?name=example.com" \
  -H "Authorization: Bearer <CF_API_TOKEN>" \
  -H "Content-Type: application/json"
```

응답의 `result[0].id` 값이 `cloudflare_zone_id`입니다.

#### `cloudflare_api_token` 발급

1. Cloudflare Dashboard → `My Profile` → `API Tokens`
2. `Create Token`
3. 템플릿은 `Edit zone DNS`를 기준으로 생성
4. 권한 확인:
   - `Zone - DNS - Edit`
   - `Zone - Zone - Read`
5. Zone Resources는 운영 도메인만 선택 (`Include - Specific zone - example.com`)
6. 생성 후 토큰을 안전하게 보관 (재조회 불가)

권장:

- 운영용 토큰은 Tunely 전용으로 분리
- 필요한 최소 권한만 부여

## 2) Relay 설정 예시

기본(경로 기반 `/t/<tunnel_id>`만 사용):

```yaml
listen: "0.0.0.0:8080"
auth_tokens:
  - "xxx"
request_timeout_secs: 60
```

동적 서브도메인 모드:

```yaml
listen: "127.0.0.1:8080"
auth_tokens:
  - "xxx"
request_timeout_secs: 60

enable_dynamic_subdomain: true
base_domain: "example.com"
cloudflare_api_token: "<CF_API_TOKEN>"
cloudflare_zone_id: "<CF_ZONE_ID>"
public_origin: "1.2.3.4"
caddy_admin_url: "http://127.0.0.1:2019"
caddy_upstream: "127.0.0.1:8080"
```

## 3) Relay CLI 플래그

`relay-server` 또는 `tunely relay`에서 사용:

```bash
--listen
--auth-token
--request-timeout-secs
--enable-dynamic-subdomain
--base-domain
--cloudflare-api-token
--cloudflare-zone-id
--public-origin
--caddy-admin-url
--caddy-upstream
```

`--enable-dynamic-subdomain`는 값 없이 사용하면 `true`로 처리됩니다.

## 4) Agent CLI 설정

| 플래그 | 기본값 | 설명 |
|---|---|---|
| `--relay` | (필수) | Relay WebSocket URL |
| `--tunnel-id` | (필수) | 터널 식별자 |
| `--token` | (필수) | relay 인증 토큰 |
| `--local` | (필수) | 프록시할 로컬 서비스 |
| `--request-subdomain` | `false` | 동적 서브도메인 자동 발급 요청 |
| `--ping-interval-secs` | `20` | ping 간격 |
| `--max-backoff-secs` | `30` | 재연결 최대 백오프 |

`--request-subdomain` 사용 시 `tunnel_id`는 DNS 라벨 규칙을 만족해야 합니다.

- 허용: 소문자, 숫자, `-`
- 비허용: 대문자, `_`, 공백

## 5) 동작 요약 (동적 모드)

1. agent가 `--request-subdomain`으로 등록
2. relay가 `<tunnel_id>.<base_domain>` DNS 레코드를 Cloudflare에 생성/갱신
3. relay가 Caddy Admin API에 host route를 생성
4. agent disconnect 시 DNS/route를 삭제

관련 문서:

- Caddy 구성: [caddy.md](caddy.md)
- 문제 해결: [troubleshooting.md](troubleshooting.md)
