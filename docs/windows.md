# Windows 실행 가이드

지원 아키텍처:

- `x64` (`x86_64`)

## 1) 배포 아티팩트로 바로 실행

GitHub Actions 아티팩트 `tunely-windows-x64.zip`을 다운로드 후 압축 해제합니다.

압축 해제 후 파일:

- `relay-server.exe`
- `agent.exe`

## 2) Relay 서버 실행 (PowerShell)

```powershell
.\relay-server.exe --listen 0.0.0.0:8080 --auth demo=xxx
```

## 3) Agent 실행 (PowerShell)

```powershell
.\agent.exe `
  --relay ws://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/ws `
  --tunnel-id demo `
  --token xxx `
  --local http://127.0.0.1:3000
```

## 4) 외부 접근 확인

```powershell
curl.exe -v http://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/t/demo/
```

바이너리 응답 확인:

```powershell
curl.exe -v http://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/t/demo/image.png --output out.png
```

## 5) Windows 방화벽

relay를 외부에서 접근해야 하면 인바운드 `TCP 8080` 허용이 필요합니다.

## 6) 자주 발생하는 문제

- `404 Not Found`: 경로를 `http://<relay>/t/<tunnel_id>/...` 형식으로 호출했는지 확인
- `502 Bad Gateway`: 해당 `tunnel_id` agent 미연결 상태
- `register rejected`: relay `--auth`와 agent `--tunnel-id/--token` 불일치
- 포트 충돌: 다른 포트 사용 또는 기존 프로세스 종료
