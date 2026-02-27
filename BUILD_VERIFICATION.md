# Build Verification - Active Targets

Verified on: 2026-02-27

## 1. Native Terminal Mode

**Binary**: `nearx`

```bash
cargo check --bin nearx --features native
cargo test --bin nearxd
```

## 2. Explorer Frontend (React/Vite)

**Workspace**: `explorer-frontend`
**Path**: `web/`

```bash
yarn workspace explorer-frontend build
```

Expected output:

- `web/dist/index.html`
- `web/dist/assets/*`

## 3. Tauri Desktop App (v2)

**Binary**: `nearx-tauri`
**Frontend dist**: `../../web/dist`
**Dev URL**: `http://127.0.0.1:1420`

```bash
cargo check --manifest-path tauri-workspace/src-tauri/Cargo.toml
cargo check --manifest-path tauri-workspace/src-tauri/Cargo.toml --features e2e
```

Key config assertions in `tauri-workspace/src-tauri/tauri.conf.json`:

- `beforeDevCommand`: Yarn workspace Vite dev command with `VITE_TAURI=true`
- `beforeBuildCommand`: Yarn workspace build command with `VITE_TAURI=true`
- `withGlobalTauri`: `false` (production hardening)

In `tauri.conf.test.json`:

- `withGlobalTauri`: `true` (E2E harness compatibility)

## 4. Yarn Workspace Baseline

```bash
yarn --version
yarn install
```

Expected:

- Yarn Berry 4.x
- single root `yarn.lock`
- no active `package-lock.json`

## Summary

Active web runtime is now the explorer frontend under `web/`; legacy `nearx-web-dom` implementation is archived under `archive/legacy-web-dom/`.
