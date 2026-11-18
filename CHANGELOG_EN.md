# Changelog

[中文](CHANGELOG.md) | [English](CHANGELOG_EN.md)

All notable changes to this project are documented in this file.

## 1.1.0 — 2026-11-18

Adds a clean uninstall, plus three fixes for the script edition under Windows PowerShell 5.1.

### GUI tool

- **Clean uninstall**: removes Logi Options+ from the system in one click. The result is judged against the real state of the registry and install directory — the official uninstaller hands the actual removal to a background process and returns 0 immediately, so the tool polls until the program is really gone (measured at a few seconds up to about 20), and only retries when nothing happens
- **A correction to an earlier assumption**: what 1.0.0 described as "the official uninstaller occasionally does nothing" was this asynchronous behaviour — the check simply ran too early
- **Optional full cleanup**: also removes key settings, the updater state and the download cache; the admin-owned folder under `%ProgramData%` is deleted with a separate elevation
- **Command line**: `--uninstall-plan` shows the plan without touching the system, `--uninstall` performs it, `--purge` removes leftovers too

### Script edition

- **About 3x faster downloads on Windows PowerShell 5.1**: the progress-bar redraw of `Invoke-WebRequest` is disabled (50 MB installer measured at 39s → 12s), and it no longer floods the console with thousands of progress lines
- **Automatic mirror fallback**: if the primary download source fails, the other official source is tried instead of aborting
- **`-UseBasicParsing` for region detection**: no longer depends on the Internet Explorer engine
- **`quiet` in the confirmation screen is no longer blank** — it shows `Yes` / `No`

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
