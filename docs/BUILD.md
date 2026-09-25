# Building SkinDocJyotsna Installers

One command builds the installer for the OS you run it on:

```bash
node scripts/build-release.mjs            # runs all tests first, then builds
node scripts/build-release.mjs --skip-tests
node scripts/build-release.mjs --bundle <offline-bundle-folder>   # also add it to a setup bundle
```

| Built on | Output | Installs to |
|---|---|---|
| Windows | `dist/release/<version>/SkinDocJyotsna_<version>_x64-setup.exe` | `C:\Program Files\SkinDocJyotsna` (all Windows users) |
| macOS | `dist/release/<version>/SkinDocJyotsna_<version>_universal.dmg` | `/Applications/SkinDocJyotsna.app` (Apple Silicon + Intel) |

Each installer gets a `.sha256` file next to it. **Installing or upgrading never touches clinic data.** Data lives in the per-user app-data folder (see the root README).

## Prerequisites

Set up the machine with the offline bundle first (`setup/README.md`, role **Dev**), then install the npm packages:

```bash
npm ci                                                     # with internet
node scripts/offline-deps.mjs restore <bundle>/project-deps  # without internet
```

## Windows

1. Open a terminal in the project folder.
2. Run `node scripts/build-release.mjs`.
3. **First build only:** the Tauri bundler downloads its installer tools (NSIS) from GitHub, so it needs internet. After that, running `node scripts/offline-deps.mjs vendor <bundle>/project-deps` copies those tools into the bundle, and later builds work offline.

## macOS

A Mac build must run on a Mac. There are two options:

**A. Build by hand on a Mac**
```bash
# once: set up the Mac with Install-DevMac.command, copy the project folder over, then
npm ci                       # or: node scripts/offline-deps.mjs restore <bundle>/project-deps
node scripts/build-release.mjs
```

**B. GitHub Actions** (once the project is in a GitHub repository): run the `build` workflow manually, or push a tag `vX.Y.Z`. Download the `.dmg` and `.exe` from the run's artifacts.

### Signing (not done yet: DESIGN Q15)

- **macOS:** the app is only *ad-hoc* signed (`signingIdentity: "-"`). Apple Silicon needs at least that to run the app at all. Gatekeeper still blocks it on first open, and `Install-ClinicMac.command` clears that flag automatically. Proper signing and notarisation need an Apple Developer ID.
- **Windows:** the `.exe` is unsigned, so SmartScreen may show "Windows protected your PC". Click **More info → Run anyway**, or install through `Install-ClinicPC.cmd`. A code-signing certificate removes the warning.

## Development vs production builds

| | `node scripts/build-release.mjs` | `node scripts/build-release.mjs --release` |
|---|---|---|
| Use for | Testing on your own PC | Installers that go to clinics |
| Machine | Any Dev-role PC | A PC set up with `Install-BuildPC.cmd` / `Install-BuildMac.command` |
| Node.js / Rust | Whatever is installed | **Must equal `setup/<os>/tools.lock`** |
| Lock check | — | `check-locks.mjs --strict`: stable releases only, every package has a checksum, `Cargo.lock` and `tools.lock` must exist |
| Cargo | normal | `--locked` (fails rather than changing `Cargo.lock`) |
| Tests | yes (`--skip-tests` allowed) | always |

Every build writes `dist/release/<version>/BUILDINFO-<os>.txt`, which records the tool versions, the installer's SHA-256, and the SHA-256 of `package-lock.json`, `Cargo.lock` and `tools.lock`. Any installer can be traced back to exactly what built it.

## Releasing a new version

1. Bump `"version"` in `package.json`. Tauri reads it from there, and so do the installer file names.
2. Build with `--release` on a Build-role Windows PC and a Build-role Mac.
3. Add both installers to the offline bundle: `--bundle <folder>`.
4. Upgrade clinic PCs by double-clicking `Install-ClinicPC.cmd` / `Install-ClinicMac.command` again. The newest version in the bundle is installed; data is kept.

## Troubleshooting

| Problem | Fix |
|---|---|
| `Rust is not installed` | Run `Install-DevPC.cmd` / `Install-DevMac.command`, then open a new terminal |
| `Rust target x86_64-apple-darwin is missing` | Re-run `Install-DevMac.command` (it adds both Mac targets) |
| Windows build stuck downloading NSIS | Needs internet on the first build, or restore `tauri-tools-windows` from the bundle |
| `Expected one .exe for version …` | Old builds are in `target/`. Delete `target/release/bundle` and build again |
