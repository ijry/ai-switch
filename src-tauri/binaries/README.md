# Built sidecar binaries for Tauri externalBin.

Tauri resolves `binaries/ai-switch-tsnet` to a target-triple-specific file during bundling.

Examples:

```text
ai-switch-tsnet-x86_64-pc-windows-msvc.exe
ai-switch-tsnet-x86_64-apple-darwin
ai-switch-tsnet-aarch64-apple-darwin
ai-switch-tsnet-x86_64-unknown-linux-gnu
```

Generate a local Windows binary before packaging:

```powershell
cd sidecar/ai-switch-tsnet
go build -o ..\..\src-tauri\binaries\ai-switch-tsnet-x86_64-pc-windows-msvc.exe .
```

The release workflow detects the Rust host target triple and writes the sidecar binary to the matching filename before `tauri build` runs.
