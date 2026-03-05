# 빌드 및 릴리즈

## 지원 타겟

- Ubuntu `amd64` / `arm64`
- Windows `x64`

## 로컬 빌드

```bash
cargo build --release -p tunely -p relay-server -p agent
```

출력 파일:

- `target/release/tunely`
- `target/release/relay-server`
- `target/release/agent`

## Ubuntu 크로스 빌드 (amd64)

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target x86_64-unknown-linux-musl -p tunely -p relay-server -p agent
```

출력 파일:

- `target/x86_64-unknown-linux-musl/release/tunely`
- `target/x86_64-unknown-linux-musl/release/relay-server`
- `target/x86_64-unknown-linux-musl/release/agent`

## Ubuntu 크로스 빌드 (arm64)

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target aarch64-unknown-linux-musl -p tunely -p relay-server -p agent
```

출력 파일:

- `target/aarch64-unknown-linux-musl/release/tunely`
- `target/aarch64-unknown-linux-musl/release/relay-server`
- `target/aarch64-unknown-linux-musl/release/agent`

## Windows 빌드 (x64)

```powershell
cargo build --release --target x86_64-pc-windows-msvc -p tunely -p relay-server -p agent
```

출력 파일:

- `target/x86_64-pc-windows-msvc/release/tunely.exe`
- `target/x86_64-pc-windows-msvc/release/relay-server.exe`
- `target/x86_64-pc-windows-msvc/release/agent.exe`

## GitHub Actions 릴리즈

워크플로: `.github/workflows/release-binaries.yml`

트리거:

- `release` 이벤트의 `published` 타입
- 수동 `workflow_dispatch`

Release 발행 시 아래 에셋을 GitHub Release에 업로드합니다:

- `tunely-linux-amd64.tar.gz`
- `tunely-linux-arm64.tar.gz`
- `tunely-windows-x64.zip`

Linux 아카이브 포함 항목:

- `tunely`
- `relay-server`
- `agent`
- `install-relay.sh`
- `install-agent.sh`
- `uninstall-tunely.sh`

Windows 아카이브 포함 항목:

- `tunely.exe`
- `relay-server.exe`
- `agent.exe`
