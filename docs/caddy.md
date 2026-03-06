# Caddy 설정 가이드 (Relay 앞단)

`relay-server` 앞에 Caddy를 두면 TLS(HTTPS/WSS)와 도메인 처리를 Caddy가 담당하고,
relay는 내부 포트(`127.0.0.1:8080`)만 열어 운영할 수 있습니다.

핵심 분리:

- 에이전트 제어 채널: `wss://relay.example.com/ws`
- 외부 HTTP/WS 터널 트래픽: `https://<tunnel>.example.com/...`

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

## 3) Caddyfile 작성 (서브도메인 라우팅)

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

동작 예시:

- `https://demo.example.com/api` -> relay `/t/demo/api`
- `wss://demo.example.com/ws` -> relay `/t/demo/ws`

적용:

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

## 4) Agent 연결 주소

```bash
tunely agent \
  --relay wss://relay.example.com/ws \
  --tunnel-id demo \
  --token xxx \
  --local http://127.0.0.1:3000
```

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

- 트러블슈팅: [troubleshooting.md](troubleshooting.md)
- Ubuntu 설치/실행: [ubuntu.md](ubuntu.md)
