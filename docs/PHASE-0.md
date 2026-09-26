| 0.5 | Installers: Windows `.exe` (NSIS, all users), macOS universal `.dmg` | ✅ Done | Built in CI, published as GitHub Release v0.1.0 with checksums; installed and launched on the MacBook |

Goal: prove every risky building block works on Windows and macOS before any features are built.

| # | Task | Status | Verified how |
|---|---|---|---|
| 0.1 | Offline setup bundle scripts (Windows + macOS) | ✅ Done | Windows bundle prepared and installed on the dev laptop (Dev/Build roles, tools.lock). macOS bundle not needed yet (CI builds) |
| 0.2 | App skeleton (Tauri + React + Rust workspace), offline project dependencies | ✅ Done | Compiles in CI on Windows + macOS; 22 UI tests |
| 0.3 | Database: auto-create + migrations, WAL/FULL, integrity check, FTS5, backup, restore | ✅ Done | 9 Rust tests pass in CI; database created on first launch on the MacBook |
| 0.4 | PDF receipt (A5, ₹ glyph, pagination) + print-ready HTML | ✅ Done | 5 Rust PDF tests + 4 preview tests pass; real-printer test in Phase 5 |
| 0.5 | Installers: Windows `.exe` (NSIS, all users), macOS universal `.dmg` | ✅ Done | Built in CI, published as GitHub Release v0.1.0 with checksums; installed and launched on the MacBook |
| 0.6 | Rebuild-from-scratch drill: setup → install → restore backup, timed | ➡️ Moved to Phases 8–9 | Needs a clean test PC and the real restore screen (see PHASE-0-REPORT.md) |
| 0.7 | Go/no-go report | ✅ GO | `docs/PHASE-0-REPORT.md` |

## Manual checks

The development laptop cannot run unsigned programs (DEC-021). Manual checks happen on the MacBook and on a Windows PC without company security software. See the 5 checks in `docs/GETTING-STARTED.md` (W7 / M7).

## Resolved
- **Q-P2: Shared data across OS accounts?** No. The clinic uses one OS login, with two app logins (ADMIN owner, RECEPTIONIST). See DEC-017. The data location stays as designed.

## Known follow-ups
- **Offline builds of the Windows installer:** handled. `offline-deps.mjs vendor` copies the Tauri NSIS tool cache once one online build has populated it.
- **Logging:** errors go to stderr for now; the rotating file logger arrives in Phase 1.
- **Permissions:** backup, restore and the System screen become admin-only once login exists (Phase 1).
