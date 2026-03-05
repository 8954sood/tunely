# Caddy 설정 가이드 (Relay 앞단)

`relay-server` 앞에 Caddy를 두면 TLS(HTTPS/WSS)와 도메인 처리를 Caddy가 맡고,
relay는 내부 포트(`127.0.0.1:8080`)에서만 동작하게 운영할 수 있습니다.

권장 흐름:

- Client -> `https://tunnel.example.com` (Caddy)
- Caddy -> `http://127.0.0.1:8080` (`relay-server`)
- Agent -> `wss://tunnel.example.com/ws`

## 1) Relay를 내부 포트로 실행

```bash
./relay-server --listen 127.0.0.1:8080 --auth demo=xxx
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

## 3) Caddyfile 작성

`/etc/caddy/Caddyfile`

```caddy
# HTTP -> HTTPS redirect는 Caddy 기본 동작
# 도메인이 이 서버를 가리키고 있어야 자동 TLS 발급 가능

tunnel.example.com {
  encode zstd gzip

  # WebSocket endpoint (agent 연결)
  reverse_proxy /ws* 127.0.0.1:8080

  # Public tunnel endpoint (client 요청)
  reverse_proxy /t/* 127.0.0.1:8080

  # 선택: access log
  log {
    output file /var/log/caddy/tunnel-access.log
    format console
  }
}
```

적용:

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

## 4) Agent 연결 주소 변경

Caddy를 쓰면 agent는 relay에 직접 `ws://127.0.0.1:8080/ws`로 붙지 않고,
도메인 기반 `wss://.../ws`로 연결합니다.

```bash
./agent \
  --relay wss://tunnel.example.com/ws \
  --tunnel-id demo \
  --token xxx \
  --local http://127.0.0.1:3000
```

외부 클라이언트 접근:

```bash
curl -v https://tunnel.example.com/t/demo/
```

## 5) 방화벽 권장

- 외부 오픈: `80/tcp`, `443/tcp`
- relay 내부포트 `8080`은 외부 미오픈 권장

예시:

```bash
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw deny 8080/tcp
```

## 6) 자주 하는 실수

- `/ws` 프록시 누락: agent가 연결 실패
- `/t/*` 프록시 누락: 클라이언트 요청 404/502
- 도메인 DNS 미설정: TLS 발급 실패
- relay를 `0.0.0.0:8080`로 열어두고 방화벽 미설정: 불필요한 노출
