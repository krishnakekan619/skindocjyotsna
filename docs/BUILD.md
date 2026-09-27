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

**B. GitHub Actions** (once the project is in a GitHub repository): run the `build` workflow manually (**Actions → build → Run workflow**), or push a tag `vX.Y.Z`. Download the `.dmg` and `.exe` from the run's artifacts.

An ordinary push to `main` runs only the Linux **check** and **audit** jobs, not the installer builds (DEC-040: Windows minutes count 2× and macOS 10×). Before tagging a release, run the workflow manually once, so Windows- or macOS-only problems show up before the tag.

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

## Releasing a new version (GitHub Actions: the normal way, DEC-021/DEC-022)

1. Bump `"version"` in `package.json` (e.g. `0.2.0`) and commit.
2. Tag and push:
   ```bash
   git tag v0.2.0
   git push origin main v0.2.0
   ```
3. The **build** workflow runs the Linux check, then builds and tests both installers. The **release** job then:
   - checks that every installer matches its `.sha256`;
   - publishes **GitHub Release `v0.2.0`** with the `.exe`, `.dmg`, `.sha256` files, `BUILDINFO` files and a combined `SHA256SUMS`.

   Releases are **kept permanently**; build artifacts are deleted after 14 days. The run fails if the tag doesn't match `package.json`.
4. Download the files from the repository's **Releases** page, while signed in, because the repository is private.

### Getting a release onto a clinic PC

**Online clinic PC:** run the `.exe` / open the `.dmg` directly.

**Offline clinic PC, or the safest route:**
1. Add the downloaded installer to the offline bundle. This checks it against its `.sha256` first:
   ```bash
   node scripts/import-release.mjs --bundle <bundle-folder> <downloaded .zip, folder or installer>
   ```
   You can give it a build-artifact `.zip` (e.g. `SkinDocJyotsna-windows-latest.zip`) or release files. Keep the `.sha256` next to each installer. Only installers for the bundle's platform are taken, and nothing changes if a checksum fails.
2. On the clinic PC, double-click `Install-ClinicPC.cmd` / `Install-ClinicMac.command`. It re-checks every checksum, installs WebView2 if it's missing, and installs the **newest** app version in the bundle. **Clinic data is kept** on upgrade.

### Local builds (alternative)
`node scripts/build-release.mjs [--release] --bundle <folder>` builds on a Build-role PC and adds the result to a bundle in one step.

## Troubleshooting

| Problem | Fix |
|---|---|
| `Rust is not installed` | Run `Install-DevPC.cmd` / `Install-DevMac.command`, then open a new terminal |
| `Rust target x86_64-apple-darwin is missing` | Re-run `Install-DevMac.command` (it adds both Mac targets) |
| Windows build stuck downloading NSIS | Needs internet on the first build, or restore `tauri-tools-windows` from the bundle |
| `Expected one .exe for version …` | Old builds are in `target/`. Delete `target/release/bundle` and build again |
