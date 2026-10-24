# Codex Switcher Local

Windows-focused hardening fork of [Lampese/codex-switcher](https://github.com/Lampese/codex-switcher). Upstream authorship and history are preserved.

This fork keeps account management, usage display, and explicit close/switch/reopen controls. It removes the LAN dashboard, account exports and backups, and automatic updates.

## Account storage

- App identity: `com.nunomacedo.codex-switcher-local`.
- Windows credentials: `%USERPROFILE%\.codex-switcher-local\accounts.dpapi`, protected with Windows DPAPI for the current user.
- Account changes use a cross-process file lock and an atomic replacement. An invalid file fails visibly instead of being reset.
- Only one desktop instance can run for a user, so two app windows cannot rotate the same refresh token concurrently.
- No credentials are automatically copied from the upstream app.
- Non-Windows storage retains a JSON file with restrictive Unix permissions; DPAPI hardening is Windows-specific.
- Codex's own `auth.json` remains in its official format. It is changed only when explicitly switching, after checking that Codex is closed.
- DPAPI protects data at rest. Software already running as the same Windows user can still access decrypted credentials.

The original `%USERPROFILE%\.codex-switcher\accounts.json` is never rewritten by migration. Existing tools that read that file continue to use it; they do not automatically follow this fork's encrypted store.

## Import existing accounts

Choose **Import existing accounts** from the app menu and select the upstream `accounts.json`. Existing entries are preserved and duplicates are skipped. The source is read-only, and migration does not close or switch Codex. If its saved active account matches the current Codex login, the latest live tokens are captured locally.

For a migration without launching the UI:

```powershell
.\src-tauri\target\debug\codex-switcher.exe --import-legacy-accounts "$env:USERPROFILE\.codex-switcher\accounts.json"
```

This command only prints imported/skipped counts. Never upload account files, authentication files, or credentials to this repository.

## Build and check

Requires Node.js 22+, pnpm 10, Rust 1.89+, the Windows MSVC build tools, and WebView2.

```powershell
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo build --locked --manifest-path src-tauri/Cargo.toml
```

Tests use synthetic credentials and temporary homes. The test suite covers Windows DPAPI round trips and tampering, concurrent account writes, failed file replacement, migration isolation, and the absence of browser account access.

No automatic release workflow or upstream updater is enabled. Do not use upstream installers to update this fork.

## Status and provenance

This is a development fork. Source checks and local tests do not clear the original Defender detection or certify a compiled executable as safe.

The upstream project has no explicit license for its own code in the reviewed snapshot. No replacement license is asserted here, and compiled releases are not published.
