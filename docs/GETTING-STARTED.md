# Getting Started: Install, Compile and Run (from scratch)

This guide takes a **blank Windows or Mac machine** to a running SkinDocJyotsna app, and then to an installer you can put on a clinic PC.

> **Current stage: Phase 0.** The Rust code has never been compiled yet, so the **first compile (step W6 / M6) is expected to show some errors**. That's normal at this stage. Copy the output and send it to Claude to fix. Everything before that step has been checked.

Timings are estimates and depend on the PC and the internet speed.

---

## What you need

| | Windows | macOS |
|---|---|---|
| OS | Windows 10 or 11, 64-bit | macOS 12 (Monterey) or newer, Apple Silicon or Intel |
| Rights | An account that can approve **administrator** prompts | Your Mac password (admin account) |
| Free disk | **~20 GB** (bundle 4–6 GB + build tools ~6 GB + build output ~3 GB) | **~10 GB** |
| Internet | For preparing the bundle and the **first** compile. Everything after that can be offline | Same |
| Apple ID | — | Free Apple ID, to download the Xcode Command Line Tools |

---

# Part 1: Windows

### W1. Put the project on the PC (2 min)
Copy the project folder to a short path without spaces, for example `C:\dev\skindocjyotsna`.
When copying from another PC, **leave out** `node_modules`, `target` and `dist`. They are large and get recreated.

Open **Windows Terminal / PowerShell** in that folder:
```powershell
cd C:\dev\skindocjyotsna
```

### W2. Prepare the offline setup bundle (15–40 min, needs internet)
Do this on any PC that has internet. It can be the same PC.

1. Double-click **`setup\windows\Prepare-OfflineBundle.cmd`**.
2. Click **Yes** when Windows asks for administrator permission. The Visual Studio Build Tools download needs it.
3. Wait for **`Bundle ready: …\dist\SkinDocJyotsna-OfflineBundle-win`**.

You can write the bundle straight to a USB drive instead:
```powershell
powershell -ExecutionPolicy Bypass -File setup\windows\Prepare-OfflineBundle.ps1 -OutputDir E:\SkinDocJyotsna-OfflineBundle-win
```
> If the PC has Node.js, the bundle also gets the project's npm packages (`project-deps`). The Rust crates are added later, in step W9, once Rust is installed.

> **Versions are locked.** The first Prepare run writes `setup\windows\tools.lock`, which records the exact version and checksum of every installer. Keep that file with the code. Later runs download exactly those versions. To move to newer stable releases, run `Prepare-OfflineBundle.ps1 -UpdateLock`.

### W3. Install the developer tools (10–25 min, no internet needed)
1. Open the bundle folder and double-click **`Install-DevPC.cmd`**.
   - Tools you **already have** (Node ≥ 24, Git ≥ 2.40, Rust ≥ 1.95) are kept, not reinstalled.
   - For a **production build PC**, use **`Install-BuildPC.cmd`** instead. Node.js and Rust must then match `tools.lock` exactly.
2. Click **Yes** at the administrator prompt.
3. Wait. A Visual Studio progress window appears for a while. That's normal.
4. The script ends with a list. **Every line should be `[OK]`:**
   ```
   [OK]   WebView2 Runtime   …
   [OK]   Git                …
   [OK]   Node.js            v24.x
   [OK]   npm                …
   [OK]   MSVC Build Tools   C:\Program Files (x86)\Microsoft Visual Studio\…
   [OK]   Windows SDK        10.0.…
   [OK]   rustc              rustc 1.x
   [OK]   cargo              cargo 1.x
   ```
5. If it says **"A restart is needed"**: restart, then double-click `Install-DevPC.cmd` again. It skips what's already done.
6. If something shows `[FAIL]`: note the log path it prints (`<bundle>\logs\install-dev-….log`), fix the cause (see Troubleshooting), and run it again.

### W4. Open a NEW terminal and check (1 min)
The old terminal doesn't see the new tools. Close it and open a new one:
```powershell
cd C:\dev\skindocjyotsna
node -v        # v24.x
rustc -V       # rustc 1.x
cargo -V       # cargo 1.x
```

### W5. Install the project's npm packages (1–3 min)
With internet:
```powershell
npm ci
```
Without internet, from the bundle:
```powershell
node scripts\offline-deps.mjs restore "E:\SkinDocJyotsna-OfflineBundle-win\project-deps"
```

### W6. Compile and run the tests (first time: 5–15 min, needs internet)
```powershell
npm test                  # UI tests: expect "22 passed"
cargo test --workspace    # Rust: downloads crates and compiles everything the first time
```
- **Expected at Phase 0:** `cargo test` may stop with compile errors. Copy the whole output (or the first ~100 lines of errors) and send it to Claude, then repeat this step after the fixes.
- When it works, you'll see `test result: ok` for `clinic-core`, `clinic-sqlite`, `clinic-pdf` and the app shell.

### W7. Run the app (first time: 3–8 min to compile)
```powershell
npm run dev
```
A window titled **SkinDocJyotsna** opens. Check these five things:

| # | Do this | Expected |
|---|---|---|
| 1 | Look at **System status** | Chips: *Integrity OK*, *WAL OK*, *Fast search (FTS5) OK*. The database file is under `%LOCALAPPDATA%\in.skindocjyotsna.clinic\data\clinic.db` |
| 2 | Click **Back up now** | A green message and a new row in the list |
| 3 | Click **Restore…**, type `RESTORE`, confirm | A green message; a **Before restore** row appears too |
| 4 | Click **Save PDF** | Message with the file path. Open it: A5 page, ₹ sign, *Sunscreen SPF 50* shown in red as *Not supplied* |
| 5 | Click **Print**, choose **Microsoft Print to PDF** | The print preview shows an A5 receipt (no real printer needed) |

To stop the app, close its window, or press `Ctrl + C` in the terminal.

> **Testing without touching real data:** point the app at a throw-away folder:
> ```powershell
> $env:SKINDOC_DATA_DIR = "C:\temp\skindoc-test"; npm run dev
> ```
> Delete `C:\temp\skindoc-test` afterwards.

### W8. Build the Windows installer (first time: 5–10 min, needs internet once)
```powershell
node scripts\build-release.mjs              # development build
node scripts\build-release.mjs --release    # production build: exact locked Node/Rust, all locks checked
```
- This runs all tests, then builds **`dist\release\0.1.0\SkinDocJyotsna_0.1.0_x64-setup.exe`**, with a `.sha256` file and **`BUILDINFO-windows.txt`** next to it. The build-info file records the exact tool versions and lock-file hashes used.
- `--release` refuses to build if Node.js or Rust differ from `tools.lock`, or if any lock contains a pre-release or a missing checksum.
- The first build downloads the NSIS installer tool once (needs internet).

### W9. Save everything for future offline builds (2–5 min, needs internet)
Now that Rust is installed, add the Rust crates and the NSIS tool to the bundle:
```powershell
node scripts\offline-deps.mjs vendor "E:\SkinDocJyotsna-OfflineBundle-win\project-deps"
```
From now on, the bundle can rebuild a developer PC and compile the app **with no internet at all**.

### W10. Install on a clinic PC (2 min)
1. Put the installer into the bundle:
   ```powershell
   node scripts\build-release.mjs --skip-tests --bundle "E:\SkinDocJyotsna-OfflineBundle-win"
   ```
2. On the clinic PC: double-click **`Install-ClinicPC.cmd`** in the bundle and click **Yes** at the prompt. It installs WebView2 if it's missing, then SkinDocJyotsna into Program Files.
3. Start **SkinDocJyotsna** from the Start menu.

   *(You can also double-click the `.exe` directly. Because the installer isn't code-signed yet, Windows may say "Windows protected your PC". Click **More info → Run anyway**.)*

---

# Part 2: macOS

### M1. Put the project on the Mac
Copy the project folder, for example to `~/dev/skindocjyotsna`, leaving out `node_modules`, `target` and `dist`. Open **Terminal**:
```bash
cd ~/dev/skindocjyotsna
```

### M2. Download the Xcode Command Line Tools installer (5 min, Apple ID)
1. Go to <https://developer.apple.com/download/all/> and sign in with a free Apple ID.
2. Search **"Command Line Tools for Xcode"**, pick the newest version that supports your macOS, and download the `.dmg`.

### M3. Prepare the offline bundle (10–20 min, needs internet)
```bash
bash setup/macos/prepare-offline-bundle.sh --clt-dmg ~/Downloads/Command_Line_Tools_for_Xcode_<version>.dmg
```
It ends with **`Bundle ready: …/dist/SkinDocJyotsna-OfflineBundle-macos`**, about 1–1.5 GB.

### M4. Install the developer tools (10–20 min)
1. In Finder, open the bundle folder and double-click **`Install-DevMac.command`**.
   - If macOS says it can't be opened: **right-click → Open → Open**, or run `bash install-from-bundle.sh --role dev` in Terminal.
2. Enter your Mac password when asked.
3. Every line of the final list should be `[OK]`, including **both** Rust targets (`aarch64-apple-darwin` and `x86_64-apple-darwin`).

### M5. Open a NEW Terminal window and check
```bash
cd ~/dev/skindocjyotsna
node -v && rustc -V && cargo -V
```

### M6. Install packages, compile and test
```bash
npm ci                    # or: node scripts/offline-deps.mjs restore <bundle>/project-deps
npm test
cargo test --workspace    # same note as W6: send any compile errors to Claude
```

### M7. Run the app
```bash
npm run dev
```
Do the same five checks as in **W7**. The database is under `~/Library/Application Support/in.skindocjyotsna.clinic/data/`. For check 5, pick **PDF → Save as PDF** in the print dialog.

### M8. Build the macOS installer
```bash
node scripts/build-release.mjs
```
This produces **`dist/release/0.1.0/SkinDocJyotsna_0.1.0_universal.dmg`**, one file that works on both Apple Silicon and Intel Macs.

### M9. Install on a clinic Mac
```bash
node scripts/build-release.mjs --skip-tests --bundle <path-to>/SkinDocJyotsna-OfflineBundle-macos
```
On the clinic Mac: double-click **`Install-ClinicMac.command`**. It copies the app to Applications and clears the "unidentified developer" block. Open **SkinDocJyotsna** from Launchpad.

---

# Part 3: Reference

### Where the app keeps things

| | Windows | macOS |
|---|---|---|
| Program | `C:\Program Files\SkinDocJyotsna\` | `/Applications/SkinDocJyotsna.app` |
| Database | `%LOCALAPPDATA%\in.skindocjyotsna.clinic\data\clinic.db` | `~/Library/Application Support/in.skindocjyotsna.clinic/data/clinic.db` |
| Backups | `…\in.skindocjyotsna.clinic\backups\*.clinicbak` | `…/in.skindocjyotsna.clinic/backups/` |
| PDF exports | `…\in.skindocjyotsna.clinic\exports\` | `…/in.skindocjyotsna.clinic/exports/` |

Paste `%LOCALAPPDATA%\in.skindocjyotsna.clinic` into the File Explorer address bar to open the folder. On a Mac, use Finder → Go → Go to Folder.

### Everyday commands

| Command | What it does |
|---|---|
| `npm run dev` | Run the app with live reload |
| `npm test` / `cargo test --workspace` | Run the UI / Rust tests |
| `npm run web:dev` | UI only, in a browser at http://localhost:1420 (no database; for layout work) |
| `node scripts/build-release.mjs` | Test and build the installer for this OS |

### Rebuilding a machine from scratch (disaster recovery)
1. **New or reinstalled PC:** run `Install-DevPC.cmd` / `Install-ClinicPC.cmd` from the bundle.
2. **Data:** copy your latest `.clinicbak` file into the new PC's backups folder, then open the app → **Backup & restore → Restore…**. *(A first-launch "Restore from backup" screen is planned for Phase 0.6.)*

### Troubleshooting

| Problem | Fix |
|---|---|
| "running scripts is disabled on this system" | Use the `.cmd` files, or `powershell -ExecutionPolicy Bypass -File <script>.ps1` |
| `rustc` / `cargo` / `node` "not recognized" | Open a **new** terminal (W4). Still missing → run `Install-DevPC.cmd` again and read the summary |
| `linker 'link.exe' not found` | MSVC Build Tools missing → run `Install-DevPC.cmd` again; restart if asked |
| `Port 1420 is already in use` | Another `npm run dev` is still running. Close it (or its terminal) and retry |
| Checksum mismatch during install | The bundle copy is damaged. Copy it again, or run Prepare again |
| Visual Studio Build Tools fails | Free ≥ 10 GB of disk space, restart, run again. Logs: `%TEMP%\dd_*.log` |
| Windows build stuck at "Downloading NSIS" | The first build needs internet (W8), then run W9 |
| macOS "cannot be opened … unidentified developer" | Right-click → **Open**, or install through `Install-ClinicMac.command` |
| `Rust target x86_64-apple-darwin is missing` | Run `Install-DevMac.command` again |
| `cargo test` compile errors | Expected in Phase 0. Send the output to Claude |

### What to send back after your first run
1. The **summary list** from `Install-DevPC.cmd` (W3).
2. The output of **`cargo test --workspace`** (W6), all of it if it failed.
3. Whether the **five checks** in W7 passed, and a screenshot of anything that looks wrong.
