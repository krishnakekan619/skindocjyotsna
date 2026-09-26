# Phase 0: Go/No-Go Report

**Date:** 2026-09-26 · **Release:** v0.1.0 (technical preview) · **Recommendation: GO for Phase 1**

Phase 0 set out to prove, before any features were built, that the chosen stack (Tauri 2 + React + TypeScript + SQLite, Rust core) can be built, installed and run on both Windows and macOS. It also had to show that the riskiest building blocks work: crash-safe database, backup/restore, PDF receipts, offline setup, and traceable versions.

## What is proven

| Area | Evidence |
|---|---|
| Builds on both OSs | GitHub Actions: Windows x64 and macOS universal builds pass (runs for `main` and tag `v0.1.0`) |
| Rust business logic | 20/20 Rust tests pass on macOS and Windows on the **first** compile: money/GST rounding (5), PDF receipt (5), database + backup/restore (9), backup file-name safety (1) |
| UI | 22/22 UI tests; typecheck clean; UI bundle 150 KB gzipped |
| Database | Created automatically on first launch; WAL + `synchronous=FULL`; integrity check; FTS5 fast search available in the bundled SQLite |
| Backup / restore | Verified snapshot, safety backup before restore, restore of corrupt, foreign or newer-version files refused, live data untouched on failure (automated tests) |
| PDF receipt | A5, embedded Noto Sans with ₹, Indian digit grouping, pagination, "Not supplied" lines (automated tests) |
| Installers | `SkinDocJyotsna_0.1.0_x64-setup.exe` and `SkinDocJyotsna_0.1.0_universal.dmg` published as GitHub Release v0.1.0, with SHA-256 checksums, BUILDINFO and Cargo.lock |
| Runs on macOS | Installed on the clinic MacBook from the release (checksum verified); app starts, database created |
| Offline setup | Windows bundle prepared (1.97 GB), every installer pinned in `tools.lock` with SHA-256; install roles Dev / Build / Clinic; no-admin option |
| Traceability | Exact versions pinned for every tool (`tools.lock`), npm package (`package-lock.json`, exact `package.json`) and Rust crate (`Cargo.lock`); `check-locks.mjs` rejects pre-releases and missing checksums; BUILDINFO per installer |
| Security baseline | No network ports; UI can only call whitelisted commands; strict CSP; no telemetry; `npm audit`: 0 vulnerabilities |

## Not yet proven (carried forward)

| Item | Moved to | Why |
|---|---|---|
| Manual checks on a real PC: backup/restore/PDF/print through the UI | Ongoing, at every release | macOS first run done; Windows needs a PC without company security software |
| 0.6 Rebuild-from-scratch drill (bundle → install → restore, timed) | Phase 8 (backup/restore) + Phase 9 (installers) | Needs a clean test PC and the real restore screen |
| Printing on a real printer | Phase 5 | No printer yet (DEC-004); print path is ready |
| macOS offline setup bundle (`setup/macos/tools.lock`) | Phase 9 | Not needed while builds run in CI |

## Risks and how they are handled

| Risk | Impact | Handling |
|---|---|---|
| The development laptop blocks unsigned programs (CrowdStrike): no local Rust builds, and the app can't run there | Slower fix-and-test loop | Builds run in GitHub Actions (DEC-021); UI work runs locally in a browser; apps are tested on the Mac / a test PC |
| Installers are unsigned: Gatekeeper and SmartScreen warnings | Staff confusion, extra steps | Documented bypass for now. **Buy an Apple Developer ID + Windows code-signing certificate before clinic rollout** (Q15) |
| No Windows test PC without company security software | Windows app not yet launched by a person | Needed before Phase 4 (billing) acceptance |
| Evergreen vendor downloads (WebView2, VS Build Tools) change | Prepare stops on checksum mismatch | Intended: `-UpdateLock` makes updates deliberate |

## Decision

**GO.** No blocker found in the stack. Next is **Phase 1**: login (owner + receptionist), roles, sessions and idle lock, first-run setup, clinic settings, audit log, file logging, and the main app layout with navigation.
