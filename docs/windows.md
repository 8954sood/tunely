# Windows 실행 가이드

지원 아키텍처:

- `x64` (`x86_64`)

## 1) 다운로드 및 압축 해제

GitHub Release 에셋에서 `tunely-windows-x64.zip`을 다운로드한 뒤 압축을 해제하세요.

포함 바이너리:

- `tunely.exe`
- `relay-server.exe`
- `agent.exe`

## 2) Relay 실행 (PowerShell)

```powershell
.\tunely.exe relay --listen 0.0.0.0:8080 --auth-token xxx,yyy
```

## 3) Agent 실행 (PowerShell)

```powershell
.\tunely.exe agent `
  --relay ws://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/ws `
  --tunnel-id demo `
  --token xxx `
  --local http://127.0.0.1:3000
```

동적 서브도메인 모드(선택):

```powershell
.\tunely.exe agent `
  --relay wss://relay.example.com/ws `
  --tunnel-id demo `
  --token xxx `
  --local http://127.0.0.1:3000 `
  --request-subdomain
```

연결 성공 시 agent 로그에 아래 2개 URL이 함께 표시됩니다.

- `https://relay.example.com/t/demo/`
- `https://demo.example.com/`

## 4) 동작 확인

```powershell
curl.exe -v http://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/t/demo/
curl.exe -v http://<RELAY_PUBLIC_IP_OR_DOMAIN>:8080/t/demo/image.png --output out.png
```

## 5) 방화벽

relay를 외부에 공개해야 하면 Windows 방화벽에서 인바운드 `TCP 8080`을 허용하세요.

## 관련 문서

- 빌드/릴리즈: [build-and-release.md](build-and-release.md)
- 설정 레퍼런스: [config.md](config.md)
- 트러블슈팅: [troubleshooting.md](troubleshooting.md)
