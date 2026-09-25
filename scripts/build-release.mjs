#!/usr/bin/env node
// Builds the SkinDocJyotsna installer for the OS this runs on, and collects it with checksums.
//
//   node scripts/build-release.mjs [--release] [--skip-tests] [--bundle <offline-bundle-dir>]
//
//   Windows -> NSIS installer (.exe, x64, installs for all users)
//   macOS   -> .dmg with a universal app (Apple Silicon + Intel)
//
//   --release     PRODUCTION build. Refuses to build unless: all locks pin stable releases with
//                 checksums (Cargo.lock and tools.lock must exist), and the installed Node.js and
//                 Rust are EXACTLY the versions in setup/<os>/tools.lock. Cargo runs with --locked.
//                 Use on a machine set up with Install-BuildPC.cmd / Install-BuildMac.command.
//   --skip-tests  skip npm test / cargo test (not allowed with --release)
//   --bundle DIR  also copy the installer into an offline setup bundle (installers/) and refresh
//                 its SHA256SUMS, so Install-ClinicPC.cmd / Install-ClinicMac.command pick it up
//
// Output: dist/release/<version>/<installer>, <installer>.sha256 and BUILDINFO.txt, which records
// the exact tool versions and lock-file hashes used, for traceability.

import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fail, isMac, isWindows, log, probe, probeAll, readPackageVersion, repoRoot, run, sha256File, writeBundleChecksums } from './lib/run.mjs';

const MAC_TARGETS = ['aarch64-apple-darwin', 'x86_64-apple-darwin'];
const osFolder = isWindows ? 'windows' : 'macos';
const toolsLockPath = join(repoRoot, 'setup', osFolder, 'tools.lock');

function parseArgs(argv) {
  const options = { release: false, skipTests: false, bundle: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--release') options.release = true;
    else if (argv[i] === '--skip-tests') options.skipTests = true;
    else if (argv[i] === '--bundle' && argv[i + 1]) options.bundle = resolve(argv[++i]);
    else fail(`Unknown option: ${argv[i]}\nUsage: node scripts/build-release.mjs [--release] [--skip-tests] [--bundle <dir>]`);
  }
  if (options.release && options.skipTests) fail('--skip-tests is not allowed for a --release build.');
  return options;
}

const versionOf = (text) => text?.match(/\d+\.\d+\.\d+/)?.[0] ?? null;

function readToolsLock() {
  if (!existsSync(toolsLockPath)) return {};
  return Object.fromEntries(
    readFileSync(toolsLockPath, 'utf8')
      .split(/\r?\n/)
      .map((line) => line.match(/^([A-Z0-9_]+)=(.*)$/))
      .filter(Boolean)
      .map((m) => [m[1], m[2]]),
  );
}

function preflight(options) {
  log(`Checking build tools (${options.release ? 'RELEASE: exact locked versions' : 'development build'})`);
  if (!isWindows && !isMac) fail('Release builds are made on Windows or macOS only.');
  const node = process.versions.node;
  if (Number(node.split('.')[0]) < 24) fail(`Node.js 24 or newer is required (found ${node}).`);
  const rustc = probe('rustc', ['-V']);
  if (!rustc) fail('Rust is not installed. Run the setup bundle (Install-DevPC.cmd / Install-DevMac.command) first.');
  console.log(`    node ${node} · ${rustc}`);
  if (isMac) {
    const sysroot = probe('rustc', ['--print', 'sysroot']);
    for (const target of MAC_TARGETS) {
      if (!sysroot || !existsSync(join(sysroot, 'lib', 'rustlib', target))) {
        fail(`Rust target ${target} is missing (needed for the universal app). Re-run Install-DevMac.command.`);
      }
    }
  }
  if (!existsSync(join(repoRoot, 'node_modules'))) fail('npm packages are not installed. Run "npm ci" (or offline-deps restore) first.');

  if (options.release) {
    log('Checking locks (release)');
    run('node', [join('scripts', 'check-locks.mjs'), '--strict']);
    const lock = readToolsLock();
    const expected = { 'Node.js': [lock.NODE_VERSION, node], Rust: [lock.RUST_VERSION, versionOf(rustc)] };
    for (const [tool, [locked, found]] of Object.entries(expected)) {
      if (!locked) fail(`${toolsLockPath} has no ${tool} version.`);
      if (locked !== found) fail(`Release builds need ${tool} ${locked} (tools.lock) but ${found} is installed. Use a PC set up with Install-BuildPC / Install-BuildMac.`);
      console.log(`    ${tool} ${found} matches tools.lock`);
    }
  }
}

function build(options) {
  log(isWindows ? 'Building Windows installer (NSIS)' : 'Building macOS universal .dmg');
  const args = isWindows ? ['tauri', 'build', '--bundles', 'nsis'] : ['tauri', 'build', '--bundles', 'dmg', '--target', 'universal-apple-darwin'];
  if (options.release) args.push('--', '--locked'); // cargo must not change Cargo.lock
  run('npx', args);
  const dir = isWindows
    ? join(repoRoot, 'target', 'release', 'bundle', 'nsis')
    : join(repoRoot, 'target', 'universal-apple-darwin', 'release', 'bundle', 'dmg');
  const extension = isWindows ? '.exe' : '.dmg';
  const version = readPackageVersion();
  const artifacts = existsSync(dir) ? readdirSync(dir).filter((name) => name.endsWith(extension) && name.includes(`_${version}_`)) : [];
  if (artifacts.length !== 1) fail(`Expected one ${extension} for version ${version} in ${dir}, found: ${artifacts.join(', ') || 'none'}`);
  return join(dir, artifacts[0]);
}

function buildInfo(options, installerName, installerHash) {
  const fileHash = (rel) => (existsSync(join(repoRoot, rel)) ? sha256File(join(repoRoot, rel)) : 'missing');
  let nativeTools = 'unknown';
  if (isWindows) {
    const vswhere = join(process.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)', 'Microsoft Visual Studio', 'Installer', 'vswhere.exe');
    nativeTools = `MSVC Build Tools ${probe(vswhere, ['-products', '*', '-latest', '-property', 'catalog_productDisplayVersion']) ?? 'unknown'}`;
  } else {
    const info = probeAll('pkgutil', ['--pkg-info=com.apple.pkg.CLTools_Executables']);
    nativeTools = `Xcode CLT ${info?.match(/^version: (.*)$/m)?.[1]?.trim() ?? 'unknown'}`;
  }
  return [
    '# SkinDocJyotsna build record - which exact tools and dependency locks produced this installer.',
    `APP_VERSION=${readPackageVersion()}`,
    `BUILD_TYPE=${options.release ? 'release' : 'development'}`,
    `BUILT_UTC=${new Date().toISOString()}`,
    `PLATFORM=${process.platform}-${process.arch}`,
    `INSTALLER=${installerName}`,
    `INSTALLER_SHA256=${installerHash}`,
    `NODE=${process.versions.node}`,
    `NPM=${probe('npm', ['-v']) ?? 'unknown'}`,
    `RUSTC=${probe('rustc', ['-V']) ?? 'unknown'}`,
    `CARGO=${probe('cargo', ['-V']) ?? 'unknown'}`,
    `NATIVE_TOOLS=${nativeTools}`,
    `PACKAGE_LOCK_SHA256=${fileHash('package-lock.json')}`,
    `CARGO_LOCK_SHA256=${fileHash('Cargo.lock')}`,
    `TOOLS_LOCK_SHA256=${fileHash(join('setup', osFolder, 'tools.lock'))}`,
  ].join('\n') + '\n';
}

function collect(artifact, options) {
  const version = readPackageVersion();
  const outDir = join(repoRoot, 'dist', 'release', version);
  mkdirSync(outDir, { recursive: true });
  const name = artifact.split(/[\\/]/).pop();
  const target = join(outDir, name);
  copyFileSync(artifact, target);
  const hash = sha256File(target);
  writeFileSync(join(outDir, `${name}.sha256`), `${hash}  ${name}\n`);
  writeFileSync(join(outDir, `BUILDINFO-${osFolder}.txt`), buildInfo(options, name, hash));
  log(`Installer: ${target}\n    SHA-256: ${hash}\n    Build record: ${join(outDir, `BUILDINFO-${osFolder}.txt`)}`);

  if (options.bundle) {
    const installers = join(options.bundle, 'installers');
    if (!existsSync(installers)) fail(`${options.bundle} does not look like an offline setup bundle (no installers/ folder).`);
    copyFileSync(target, join(installers, name));
    writeBundleChecksums(options.bundle);
    log(`Added to offline bundle: ${join(installers, name)} (SHA256SUMS refreshed)`);
  }
}

const options = parseArgs(process.argv.slice(2));
preflight(options);
if (!options.skipTests) {
  log('Running tests');
  run('npm', ['test']);
  run('cargo', options.release ? ['test', '--workspace', '--locked'] : ['test', '--workspace']);
}
collect(build(options), options);
if (isMac) {
  console.log('\nNote: the app is not signed with an Apple Developer ID yet. On first launch, clinic Macs');
  console.log('need Install-ClinicMac.command (it clears the quarantine flag) or right-click > Open.');
}
