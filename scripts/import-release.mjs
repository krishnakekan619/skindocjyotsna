#!/usr/bin/env node
// Adds installers built by GitHub Actions to an offline setup bundle, so a clinic PC can be set
// up with one double-click (Install-ClinicPC.cmd / Install-ClinicMac.command), even without internet.
//
//   node scripts/import-release.mjs --bundle <bundle-folder> <download> [<download> ...]
//
// <download> can be: a build-artifact .zip (e.g. SkinDocJyotsna-windows-latest.zip), a folder,
// or installer files downloaded from a GitHub Release (keep the .sha256 file next to each one).
//
// For every installer found:
//   1. it must have a matching <installer>.sha256 (from the same build) and the hash must match
//   2. only installers for the bundle's platform are taken (.exe -> Windows bundle, .dmg -> macOS)
//   3. it is copied to <bundle>/installers/, its BUILDINFO file to <bundle>/release-info/<version>/
//   4. <bundle>/SHA256SUMS is rewritten, so the install script verifies it before installing
// Nothing is changed in the bundle unless every installer passes its checksum.

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fail, isWindows, log, run, sha256File, warn, writeBundleChecksums } from './lib/run.mjs';

const INSTALLER = /^SkinDocJyotsna_(\d+\.\d+\.\d+)_(x64-setup\.exe|universal\.dmg)$/;

function parseArgs(argv) {
  const options = { bundle: null, inputs: [] };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--bundle' && argv[i + 1]) options.bundle = resolve(argv[++i]);
    else if (argv[i].startsWith('--')) fail(`Unknown option: ${argv[i]}`);
    else options.inputs.push(resolve(argv[i]));
  }
  if (!options.bundle || options.inputs.length === 0) {
    fail('Usage: node scripts/import-release.mjs --bundle <bundle-folder> <download.zip|folder|installer> [...]');
  }
  return options;
}

function bundlePlatform(bundle) {
  const manifest = join(bundle, 'manifest.txt');
  if (!existsSync(manifest) || !existsSync(join(bundle, 'installers'))) {
    fail(`${bundle} is not an offline setup bundle (manifest.txt / installers missing). Create it with Prepare-OfflineBundle first.`);
  }
  const platform = readFileSync(manifest, 'utf8').match(/^BUNDLE_PLATFORM=(.*)$/m)?.[1]?.trim();
  if (platform !== 'windows-x64' && platform !== 'macos') fail(`Unknown bundle platform '${platform}' in ${manifest}`);
  return platform;
}

/** Unzips with the system's bsdtar (Windows 10+ and macOS both ship one that reads .zip). */
function unzip(zip, into) {
  const tar = isWindows ? join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe') : 'tar';
  run(tar, ['-xf', zip, '-C', into]);
}

function listFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? listFiles(join(dir, entry.name)) : [join(dir, entry.name)],
  );
}

function collectFiles(inputs, scratch) {
  const files = [];
  inputs.forEach((input, index) => {
    if (!existsSync(input)) fail(`Not found: ${input}`);
    if (statSync(input).isDirectory()) files.push(...listFiles(input));
    else if (input.toLowerCase().endsWith('.zip')) {
      const into = join(scratch, `zip-${index}`);
      mkdirSync(into, { recursive: true });
      log(`Unpacking ${basename(input)}`);
      unzip(input, into);
      files.push(...listFiles(into));
    } else {
      // A single downloaded file: also look at its neighbours (.sha256, BUILDINFO).
      files.push(...listFiles(dirname(input)).filter((f) => f === input || f.startsWith(`${input}.`) || basename(f).startsWith('BUILDINFO-')));
    }
  });
  return files;
}

function verify(installer, allFiles) {
  const name = basename(installer);
  const sumFile = allFiles.find((f) => basename(f) === `${name}.sha256`);
  if (!sumFile) fail(`${name}: no ${name}.sha256 next to it. Download it from the same build or release.`);
  const expected = readFileSync(sumFile, 'utf8').trim().split(/\s+/)[0]?.toLowerCase();
  const actual = sha256File(installer);
  if (expected !== actual) fail(`${name}: checksum mismatch (expected ${expected}, got ${actual}). The download is damaged or not from this build.`);
  console.log(`    [OK]   ${name}  sha256 ${actual}`);
}

const options = parseArgs(process.argv.slice(2));
const platform = bundlePlatform(options.bundle);
const wanted = platform === 'windows-x64' ? '.exe' : '.dmg';
const scratch = mkdtempSync(join(tmpdir(), 'skindoc-import-'));
try {
  const files = collectFiles(options.inputs, scratch);
  const installers = files.filter((f) => INSTALLER.test(basename(f)));
  if (installers.length === 0) fail('No SkinDocJyotsna installer (.exe or .dmg) found in the given downloads.');

  log(`Verifying installers (bundle platform: ${platform})`);
  const selected = [];
  for (const installer of installers) {
    if (!installer.endsWith(wanted)) {
      warn(`${basename(installer)} is not for a ${platform} bundle; skipped.`);
      continue;
    }
    verify(installer, files);
    selected.push(installer);
  }
  if (selected.length === 0) fail(`No ${wanted} installer for this ${platform} bundle in the given downloads.`);

  log('Adding to the bundle');
  for (const installer of selected) {
    const name = basename(installer);
    const version = name.match(INSTALLER)[1];
    copyFileSync(installer, join(options.bundle, 'installers', name));
    console.log(`    installers/${name}`);
    const os = wanted === '.exe' ? 'windows' : 'macos';
    const info = files.find((f) => basename(f) === `BUILDINFO-${os}.txt`);
    if (info) {
      const infoDir = join(options.bundle, 'release-info', version);
      mkdirSync(infoDir, { recursive: true });
      copyFileSync(info, join(infoDir, basename(info)));
      console.log(`    release-info/${version}/${basename(info)}`);
    } else {
      warn(`No BUILDINFO-${os}.txt found for ${name} (optional, recommended for traceability).`);
    }
  }
  writeBundleChecksums(options.bundle);
  log(`SHA256SUMS refreshed. Next: double-click ${platform === 'windows-x64' ? 'Install-ClinicPC.cmd' : 'Install-ClinicMac.command'} in the bundle.`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
