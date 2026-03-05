# Ubuntu 실행 가이드 (Caddy 미포함)

지원 아키텍처:

- `amd64` (`x86_64`)
- `arm64` (`aarch64`)

아래 예시는 Ubuntu 22.04+ 기준입니다.

## 1) 가장 빠른 배치 (권장, GitHub에서 바로 설치)

### Relay 설치

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-relay.sh | sudo bash
```

설치 후 생성 파일:

- 실제 설정 파일(빈 파일): `/etc/tunely/relay.yaml`
- 주석 예제 파일: `/etc/tunely/relay.example.yaml`

`relay.yaml`이 비어 있으면 서비스는 자동 시작하지 않습니다.

### Relay 설정 입력

```bash
sudo cp /etc/tunely/relay.example.yaml /tmp/relay.example.yaml
sudo vi /etc/tunely/relay.yaml
```

`/etc/tunely/relay.yaml` 예시(직접 입력):

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
# 이미 실행 중이면
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

## 2) 버전 고정 설치 (선택)

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-relay.sh | \
  sudo bash -s -- --version v0.1.0
```

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-agent.sh | \
  sudo bash -s -- --relay ws://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/ws --tunnel-id demo --token xxx --local http://127.0.0.1:3000 --version v0.1.0
```

## 3) 상태/로그 확인

```bash
systemctl status tunely-relay
systemctl status tunely-agent

journalctl -u tunely-relay -f
journalctl -u tunely-agent -f
```

## 4) 외부 접근 확인

```bash
curl -v http://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/t/demo/
```

바이너리 응답 확인:

```bash
curl -v http://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/t/demo/image.png --output /tmp/out.png
```

## 5) 제거

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/uninstall-tunely.sh | sudo bash
```

## 6) 수동 실행 (스크립트 없이)

### Relay

```bash
./relay-server --config /etc/tunely/relay.yaml
```

### Agent

```bash
./agent \
  --relay ws://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/ws \
  --tunnel-id demo \
  --token xxx \
  --local http://127.0.0.1:3000
```

## 7) 방화벽

외부 공개 relay 서버라면 8080 오픈이 필요합니다.

```bash
sudo ufw allow 8080/tcp
sudo ufw status
```

## 8) 자주 발생하는 문제

- `auth_tokens must not be empty`: `/etc/tunely/relay.yaml`에 `auth_tokens`를 설정해야 함
- `404 Not Found`: 경로를 `http://<relay>/t/<tunnel_id>/...` 형식으로 호출했는지 확인
- `502 Bad Gateway`: 해당 `tunnel_id` agent 미연결 상태
- `register rejected: invalid token`: relay `auth_tokens` 목록에 agent `--token`이 없음
- `register rejected: tunnel_id already in use`: 이미 같은 `tunnel_id`가 연결 중
- `Address already in use`: 포트 충돌(기존 프로세스 종료 또는 다른 포트 사용)
