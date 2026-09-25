# SkinDocJyotsna

A local-first clinic inventory, billing and client management app for Windows and macOS.
Built with **Tauri 2 + React + TypeScript + SQLite**, with a Rust core. It works fully offline.

> **Status: Phase 0 (technical spike).** See `docs/DESIGN-v2-desktop.md` for the design and `docs/DECISIONS.md` for the decisions made so far.

## Project layout

```
src/                  React UI (MUI). Screens talk only to the ClinicApi interface (src/api).
src-tauri/            Desktop shell (Rust): IPC commands, app paths, permissions, bundling config
crates/clinic-core/   Business rules (money, pricing…): no UI, no database
crates/clinic-sqlite/ SQLite: connection settings, migrations, health checks
scripts/              Cross-platform helper scripts (offline dependencies)
setup/                One-run offline environment setup for Windows and macOS
docs/                 Design, decisions
```

**New here? Follow [`docs/GETTING-STARTED.md`](docs/GETTING-STARTED.md).** It covers a blank Windows or Mac machine through to a running app and installer, step by step.

## First-time setup

1. Install the tools with the offline bundle. See [`setup/README.md`](setup/README.md). You run `Install-DevPC.cmd` on Windows or `Install-DevMac.command` on macOS.
2. Install the project dependencies, whichever applies:
   ```bash
   npm install                                               # with internet
   node scripts/offline-deps.mjs restore <bundle>/project-deps   # without internet
   ```

## Everyday commands

| Command | What it does |
|---|---|
| `npm run dev` | Run the desktop app with hot reload |
| `npm run web:dev` | UI only in a browser at http://localhost:1420, for layout work (no database) |
| `npm run build` | Build the installer: `.exe` (NSIS) on Windows, `.dmg` on macOS → `target/release/bundle/` |
| `npm test` | UI unit tests (Vitest) |
| `npm run test:rust` | Rust unit and integration tests |
| `npm run typecheck` | TypeScript type check |
| `npm run deps:vendor -- <dir>` | Save all npm packages and Rust crates to `<dir>` for offline builds |

## Where the app keeps its data

The app creates its database automatically on first launch. There is nothing to install or configure.

| OS | Folder |
|---|---|
| Windows | `%LOCALAPPDATA%\in.skindocjyotsna.clinic\` (`data\clinic.db`, `backups\`) |
| macOS | `~/Library/Application Support/in.skindocjyotsna.clinic/` · logs in `~/Library/Logs/in.skindocjyotsna.clinic/` |

To use a different folder (for tests or recovery), set `SKINDOC_DATA_DIR`.

**Never commit database or backup files.** They contain patient data. `.gitignore` blocks `*.db` and `*.clinicbak`.
