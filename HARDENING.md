# Codex Switcher Local

Windows hardening fork of Lampese/codex-switcher. Work in progress; do not replace the installed application yet.

The fork uses a separate application identity and account directory, removes LAN mode and upstream updates, encrypts its Windows account store with DPAPI, and serializes credential changes with atomic file replacement. Backup/export features have been removed at the user's request.

The upstream account store and Codex history/settings remain unchanged during development and testing. Tests use synthetic credentials and temporary directories. One-time account import is read-only at the source and does not switch or close Codex.

The upstream project does not currently declare a license for its own code. This fork preserves upstream authorship and does not add a license or publish compiled releases.

Local Windows validation: 28 JavaScript tests and 66 Rust tests passed, along with the frontend and native builds. Migration verifies the encrypted destination while preserving the original account file and current Codex authentication. Live account switching, full desktop UI behavior, signing, and release packaging remain unverified.
