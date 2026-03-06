# Ubuntu 실행 가이드 (Caddy 미포함)

지원 아키텍처:

- `amd64` (`x86_64`)
- `arm64` (`aarch64`)

예시는 Ubuntu 22.04+ 기준입니다.

## 1) 빠른 배치 (권장)

### Relay 설치

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-relay.sh | sudo bash
```

설치 후 설정 파일:

- `/etc/tunely/relay.yaml` (주석 템플릿 포함)

파일이 주석만 있거나 비어 있으면 서비스는 enable만 되고 start는 하지 않습니다.

### Relay 설정

```bash
sudo vi /etc/tunely/relay.yaml
```

예시:

```yaml
listen: "0.0.0.0:8080"
auth_tokens:
  - "xxx"
  - "yyy"
request_timeout_secs: 60
```

적용:

```bash
sudo systemctl start tunely-relay
sudo systemctl restart tunely-relay
```

### Agent 설치

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-agent.sh | \
  sudo bash -s -- \
    --relay ws://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/ws \
    --tunnel-id demo \
    --token xxx \
    --local http://127.0.0.1:3000
```

## 2) non-systemd 환경 (Docker/WSL)

`--no-systemd`를 사용하세요:

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-relay.sh | \
  sudo bash -s -- --no-systemd

curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-agent.sh | \
  sudo bash -s -- \
    --no-systemd \
    --relay ws://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/ws \
    --tunnel-id demo \
    --token xxx \
    --local http://127.0.0.1:3000
```

수동 실행:

```bash
tunely relay --config /etc/tunely/relay.yaml
tunely agent --relay ws://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/ws --tunnel-id demo --token xxx --local http://127.0.0.1:3000
```

## 3) 버전 고정 설치

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-relay.sh | \
  sudo bash -s -- --version v0.3.0
```

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-agent.sh | \
  sudo bash -s -- \
    --relay ws://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/ws \
    --tunnel-id demo \
    --token xxx \
    --local http://127.0.0.1:3000 \
    --version v0.3.0
```

## 4) 상태 확인 / 로그

```bash
systemctl status tunely-relay
systemctl status tunely-agent
journalctl -u tunely-relay -f
journalctl -u tunely-agent -f
```

요청 테스트:

```bash
curl -v http://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/t/demo/
curl -v http://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/t/demo/image.png --output /tmp/out.png
```

## 5) 방화벽

```bash
sudo ufw allow 8080/tcp
sudo ufw status
```

## 6) 제거

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/uninstall-tunely.sh | sudo bash
```

## 관련 문서

- Caddy 설정: [caddy.md](caddy.md)
- 빌드/릴리즈: [build-and-release.md](build-and-release.md)
- 트러블슈팅: [troubleshooting.md](troubleshooting.md)
