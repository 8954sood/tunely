# Ubuntu 실행 가이드 (Caddy 미포함)

지원 아키텍처:

- `amd64` (`x86_64`)
- `arm64` (`aarch64`)

아래 예시는 Ubuntu 22.04+ 기준입니다.

## 1) 가장 빠른 배치 (권장, GitHub에서 바로 설치)

`relay-server` 설치:

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-relay.sh | \
  sudo bash -s -- --auth demo=xxx --listen 0.0.0.0:8080
```

`agent` 설치:

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-agent.sh | \
  sudo bash -s -- \
    --relay ws://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/ws \
    --tunnel-id demo \
    --token xxx \
    --local http://127.0.0.1:3000
```

스크립트가 자동으로 처리하는 항목:

- GitHub 릴리즈(`8954sood/tunely`)에서 현재 아키텍처용 바이너리 자동 다운로드
- `/opt/tunely` 바이너리 설치
- `/etc/tunely/*.env` 설정 파일 생성
- `tunely-relay.service`, `tunely-agent.service` 생성
- `systemctl enable --now`로 즉시 시작

## 2) 버전 고정 설치 (선택)

`latest` 대신 특정 태그를 고정하려면 `--version`을 지정합니다.

```bash
curl -fsSL https://raw.githubusercontent.com/8954sood/tunely/main/scripts/install-relay.sh | \
  sudo bash -s -- --auth demo=xxx --version v0.1.0
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

## 6) 수동 설치/실행 (오프라인 또는 직접 파일 사용)

릴리즈 아티팩트(`tunely-linux-amd64.tar.gz`, `tunely-linux-arm64.tar.gz`)를 직접 받아서
압축을 풀고 `./install-relay.sh`, `./install-agent.sh`를 실행해도 됩니다.

## 7) 방화벽

외부 공개 relay 서버라면 8080 오픈이 필요합니다.

```bash
sudo ufw allow 8080/tcp
sudo ufw status
```

## 8) 자주 발생하는 문제

- `404 Not Found`: 경로를 `http://<relay>/t/<tunnel_id>/...` 형식으로 호출했는지 확인
- `502 Bad Gateway`: 해당 `tunnel_id` agent 미연결 상태
- `register rejected`: relay `--auth`와 agent `--tunnel-id/--token` 불일치
- `Address already in use`: 포트 충돌(기존 프로세스 종료 또는 다른 포트 사용)
