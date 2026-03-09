# Caddy + Cloudflare 설정 가이드

`relay-server` 앞에 Caddy를 두면 TLS(HTTPS/WSS)와 도메인 처리를 Caddy가 담당하고,
relay는 내부 포트(`127.0.0.1:8080`)만 열어 운영할 수 있습니다.

핵심 분리:

- 에이전트 제어 채널: `wss://relay.example.com/ws`
- 외부 HTTP/WS 터널 트래픽: `https://<tunnel>.example.com/...`

## 모드 선택

- 정적 모드: Caddyfile의 wildcard rewrite만 사용 (`*.example.com -> /t/<tunnel_id>`)
- 동적 모드: Relay가 Cloudflare DNS + Caddy Admin API를 사용해 서브도메인을 자동 생성/삭제

`--request-subdomain`을 쓸 계획이면 동적 모드를 권장합니다.

## 1) Relay를 내부 포트로 실행

```bash
tunely relay --listen 127.0.0.1:8080 --auth-token xxx,yyy
```

## 2) Caddy 설치 (Ubuntu 예시)

```bash
sudo apt update
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https curl
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update
sudo apt install -y caddy
```

## 3) 정적 모드 (Caddyfile rewrite)

`/etc/caddy/Caddyfile`

```caddy
# relay.example.com: agent 제어 채널(/ws)
relay.example.com {
  encode zstd gzip
  reverse_proxy /ws* 127.0.0.1:8080
}

# *.example.com: 외부 사용자 HTTP/WS 트래픽
*.example.com {
  encode zstd gzip

  # subdomain -> tunnel_id 변환
  @sub host_regexp tid ^([a-z0-9-]+)\.example\.com$
  rewrite @sub /t/{re.tid.1}{uri}

  reverse_proxy 127.0.0.1:8080
}
```

적용:

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

## 4) 동적 모드 (Cloudflare + Caddy Admin API)

동작:

1. agent가 `--request-subdomain`으로 등록 요청
2. relay가 `<tunnel_id>.<base_domain>` DNS 레코드(Cloudflare, DNS only)를 생성/갱신
3. relay가 Caddy Admin API로 host match 라우트를 생성 (`/t/<tunnel_id>{uri}`로 rewrite)
4. agent disconnect 시 DNS/라우트를 삭제

### 4-1) Cloudflare 준비 (`zone_id`, API 토큰)

- `cloudflare_zone_id`: Cloudflare 대시보드에서 도메인 선택 후 `API` 섹션의 `Zone ID`
- `cloudflare_api_token`: `My Profile -> API Tokens -> Create Token`에서 발급
- 권장 권한:
  - `Zone - DNS - Edit`
  - `Zone - Zone - Read`
- Zone Resources는 대상 도메인만 포함하도록 제한 권장

상세 절차는 [config.md](config.md)의 `Cloudflare zone_id / API 토큰 발급 방법`을 참고하세요.

### 4-2) Relay 설정 예시 (`/etc/tunely/relay.yaml`)

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

### 4-3) Caddy 설정 예시 (Admin API 활성화)

`/etc/caddy/Caddyfile`

```caddy
{
  admin 127.0.0.1:2019
}

relay.example.com {
  encode zstd gzip
  reverse_proxy /ws* 127.0.0.1:8080
}
```

> 동적 모드에서는 개별 터널 host 라우트를 relay가 Admin API로 직접 추가/삭제합니다.

적용:

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

### 4-4) Agent 연결

```bash
tunely agent \
  --relay wss://relay.example.com/ws \
  --tunnel-id demo \
  --token xxx \
  --local http://127.0.0.1:3000 \
  --request-subdomain
```

연결 성공 시 agent 로그에 URL 2개가 출력됩니다.

- 경로 기반: `https://relay.example.com/t/demo/`
- 서브도메인: `https://demo.example.com/`

## 5) 외부 접근 확인

HTTP:

```bash
curl -v https://demo.example.com/
```

WebSocket(예: echo endpoint `/ws`):

```bash
# 예시 도구가 있다면 wscat/websocat 등으로 접속
wscat -c wss://demo.example.com/ws
```

## 6) 방화벽 권장

- 외부 오픈: `80/tcp`, `443/tcp`
- relay 내부포트 `8080`은 외부 미오픈 권장

```bash
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw deny 8080/tcp
```

## 7) 관련 문서

- 설정 레퍼런스: [config.md](config.md)
- 트러블슈팅: [troubleshooting.md](troubleshooting.md)
- Ubuntu 설치/실행: [ubuntu.md](ubuntu.md)
