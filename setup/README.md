# SkinDocJyotsna: Offline Environment Setup

These scripts set up a Windows PC or a Mac for SkinDocJyotsna **from a folder of installers that works without internet**. Setting up a new or rebuilt machine takes one double-click.

It works in two steps:

1. **Prepare**, once, on any machine with internet. This downloads the **locked** version of every installer into a bundle folder. Copy that folder to a USB drive or network share.
2. **Install**, on each target machine, offline. Double-click the installer for the machine's role. It checks every file's checksum, looks at what's already installed, installs only what's needed, and then verifies every tool.

Both steps are safe to repeat. Files and tools that are already correct are skipped, so after a failure you fix the cause and just run the script again.

## Roles: what gets installed, and when existing tools are kept

| Role | Double-click | Existing tools | Installs |
|---|---|---|---|
| **Dev** (developer PC) | `Install-DevPC.cmd` / `Install-DevMac.command` | **Kept** if at least the minimum version in `versions.env` (Node ≥ 24.0.0, Rust ≥ 1.90.0, Git ≥ 2.40.0) | Anything missing or too old, at the locked version |
| **Build** (makes production installers) | `Install-BuildPC.cmd` / `Install-BuildMac.command` | **Node.js and Rust must match `tools.lock` exactly.** If a different version is installed, setup stops and explains why. It is never replaced silently | Anything missing, at the locked version |
| **Clinic** (runs the app) | `Install-ClinicPC.cmd` / `Install-ClinicMac.command` | — | Windows: WebView2 (if missing) + the SkinDocJyotsna app. macOS: the app |

The developer and build tools are:
- **Windows:** WebView2, Git, Node.js 24, Visual Studio Build Tools (**only** the MSVC x64 compiler and the Windows 11 SDK), and Rust.
- **macOS:** Xcode Command Line Tools, Node.js 24, and Rust with both Apple Silicon and Intel targets.

> The SQLite database needs no separate install. SkinDocJyotsna creates the database and all its tables automatically on first launch.

## Version locks: every tool is traceable

| File | What it pins | Kept with the code? |
|---|---|---|
| `setup/versions.env` | **Policy:** which releases to pick (Node 24.x, stable Rust) and the minimum versions Dev may keep | Yes |
| `setup/windows/tools.lock`, `setup/macos/tools.lock` | **Exact** version, download URL and SHA-256 of every installer in the bundle | Yes |
| `package-lock.json`, `Cargo.lock` | Exact version and hash of every npm package and Rust crate | Yes |
| `dist/release/<ver>/BUILDINFO-<os>.txt` | Which tool versions and lock-file hashes produced each installer | With each release |

- **First Prepare run:** no lock exists yet, so it pins the **newest stable** releases and writes `tools.lock`.
- **Every later run:** downloads exactly what the lock says, and **refuses** any file whose checksum differs.
- **Upgrading tools** is deliberate: run Prepare with `-UpdateLock` / `--update-lock`, review the printed `old -> new` changes, and keep the new lock.
- **`node scripts/check-locks.mjs`** checks that every lock pins **stable** releases (no alpha/beta/rc) with checksums. It runs automatically in release builds and CI.

## Windows

### 1. Prepare the bundle (PC with internet)
Double-click `setup\windows\Prepare-OfflineBundle.cmd`, or run:
```powershell
powershell -ExecutionPolicy Bypass -File setup\windows\Prepare-OfflineBundle.ps1                      # locked versions -> dist\SkinDocJyotsna-OfflineBundle-win
powershell -ExecutionPolicy Bypass -File setup\windows\Prepare-OfflineBundle.ps1 -OutputDir E:\Bundle  # straight onto a USB drive
powershell -ExecutionPolicy Bypass -File setup\windows\Prepare-OfflineBundle.ps1 -UpdateLock           # move to newest stable releases
powershell -ExecutionPolicy Bypass -File setup\windows\Prepare-OfflineBundle.ps1 -SkipBuildTools       # clinic-only bundle (~0.4 GB)
powershell -ExecutionPolicy Bypass -File setup\windows\Prepare-OfflineBundle.ps1 -ResolveOnly          # just show versions/URLs
```
- The Build Tools layout now contains only the MSVC compiler and the Windows 11 SDK, so the bundle and install time are much smaller than a full C++ workload.
- Windows asks for administrator permission once, for the Visual Studio download.
- If a run is interrupted, run it again. Files that are already complete are kept.

### 2. Install (target PC, no internet needed)
Copy the bundle folder to the PC, then double-click `Install-DevPC.cmd`, `Install-BuildPC.cmd` or `Install-ClinicPC.cmd`.

Windows asks for administrator permission once. The summary at the end shows each tool with the version **found** and the version **locked**. A log is saved in `<bundle>\logs\`.

To check a PC without installing anything:
```powershell
powershell -ExecutionPolicy Bypass -File Install-FromBundle.ps1 -VerifyOnly -Role Build
```

## macOS

### 1. Prepare the bundle (Mac/Linux with internet)
```bash
bash setup/macos/prepare-offline-bundle.sh --clt-dmg ~/Downloads/Command_Line_Tools_for_Xcode_<ver>.dmg
bash setup/macos/prepare-offline-bundle.sh --update-lock      # move to newest stable releases
bash setup/macos/prepare-offline-bundle.sh --resolve-only
```
- **Xcode Command Line Tools** can only be downloaded by a signed-in Apple developer, and a free Apple ID is enough. Download *Command Line Tools for Xcode* from <https://developer.apple.com/download/all/> and pass it with `--clt-dmg`. Its checksum is recorded in the lock too.
- The bundle is roughly **1–1.5 GB**. It includes Rust for both Apple Silicon and Intel.

### 2. Install (target Mac)
Copy the bundle folder to the Mac, then double-click `Install-DevMac.command`, `Install-BuildMac.command` or `Install-ClinicMac.command`.

If macOS blocks a copied script, right-click it → **Open**, or run `bash install-from-bundle.sh --role dev` in Terminal. It asks for your Mac password once. Logs go to `<bundle>/logs/`.

## Security of the bundle

- **Downloads are checked against the vendor's published checksum** when a version is first pinned:
  - Node.js `SHASUMS256.txt`
  - Rust `*.sha256`
  - Git for Windows GitHub asset digest
- **After that, against `tools.lock`.**
- **Signatures:** Windows installers must carry a valid Authenticode signature, and Microsoft components must be signed by Microsoft Corporation. macOS packages must pass `pkgutil --check-signature`.
- **Evergreen downloads:** Microsoft's WebView2 and Build Tools links always serve the newest build. If Microsoft publishes a new one, the checksum no longer matches the lock and Prepare **stops**, rather than silently taking it. Accept it with `-UpdateLock`.
- **Before installing,** the install scripts re-check `SHA256SUMS`. This catches a damaged or tampered USB copy.
- **Contents:** the bundle contains **no secrets and no patient data**. Keep it apart from clinic backups, which do contain patient data.

## Troubleshooting

| Problem | Fix |
|---|---|
| "running scripts is disabled on this system" | Use the `.cmd` files, or `powershell -ExecutionPolicy Bypass -File …` |
| "… does not match tools.lock" during Prepare | The vendor changed the file behind the same link. If that's expected, run with `-UpdateLock` / `--update-lock` and review the change |
| Checksum mismatch during Install | The bundle copy is damaged. Copy it again, or re-run Prepare |
| "Build role needs Node.js X exactly" | Uninstall the other version (Apps & features), use a clean build PC/VM, or use the Dev role |
| Build Tools install fails | Look at `%TEMP%\dd_*.log`, free up ≥ 10 GB, restart, run again |
| "A restart is needed" | Restart, then run the same script again to finish and verify |
| `cargo`/`node` not found right after install | Open a **new** terminal window |
| macOS "unidentified developer" | Right-click → Open, or `bash Install-DevMac.command` in Terminal |
| macOS: Rust installed via rustup | Dev keeps it: add the other target with `rustup target add <target>`. Build needs `rustup default <locked version>` |

## Offline builds

When the project folder is present, Prepare also stores the project's npm packages (for all build platforms), Rust crates and the Tauri installer tools in `<bundle>/project-deps`. On an offline machine, run `node scripts/offline-deps.mjs restore <bundle>/project-deps`. The Rust crates and Tauri tools are only included once Prepare (or `offline-deps.mjs vendor`) has run on a machine that has Rust and has done one build.
