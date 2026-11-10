# Changelog

[中文](CHANGELOG.md) | [English](CHANGELOG_EN.md)

All notable changes to this project are documented in this file.

## 1.0.0 — 2026-11-10

First release of the Windows GUI tool.

- **Per-feature toggles**: 10 installer switches editable per item; AI Prompt Builder is shown as "always enabled by the official installer"
- **Two installation sources**: official online edition and official offline edition, each with its own default feature preset; switching sources applies the matching preset
- **Offline suggestion**: offers a one-click switch when the selection is close to the offline defaults
- **Settings backup & restore**: key/mouse customizations are backed up before installing and restored after; locked files are skipped with the backup kept for a later retry
- **Signature verification**: downloads must pass Authenticode verification (Logitech Inc) before installation; cached packages are re-verified before reuse
- **Same-version skip detection**: warns when the official installer skipped a same-version install and feature switches were not rewritten
- **Uninstall verification**: the product uninstaller may return exit code 0 without actually uninstalling; the tool verifies the result and retries once, then explains the available options
- **Version regression notice** for offline-package installs
- **Cancellation-safe**: declining the UAC prompt stops the flow cleanly with the configuration backup retained
- **Update check** against the official update manifest
- **Command-line mode**: `--check`, `--plan`, `--install` (with `--offline`, `--features`, `--reuse`, `--keep`)
- **Single 6 MB binary**, no runtime dependencies; dark/light theme follows the system
