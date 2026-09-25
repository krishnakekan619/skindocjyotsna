// Shared helpers for the project's Node scripts (Windows and macOS).

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const isWindows = process.platform === 'win32';
export const isMac = process.platform === 'darwin';

export function log(message) {
  console.log(`\n==> ${message}`);
}

export function warn(message) {
  console.warn(`[WARN] ${message}`);
}

export function fail(message) {
  console.error(`[FAIL] ${message}`);
  process.exit(1);
}

function spawn(command, args, options) {
  // npm/npx are .cmd shims on Windows and must be started through cmd.exe. Arguments are
  // fixed strings, lockfile URLs or folder paths (quoted here), never user input.
  if (isWindows && (command === 'npm' || command === 'npx')) {
    const line = [command, ...args.map((a) => `"${a}"`)].join(' ');
    return spawnSync(process.env.ComSpec ?? 'cmd.exe', ['/d', '/s', '/c', `"${line}"`], { ...options, windowsVerbatimArguments: true });
  }
  return spawnSync(command, args, options);
}

/** Runs a command in the repo root, streaming output; exits the script if it fails. */
export function run(command, args, { capture = false, cwd = repoRoot } = {}) {
  const result = spawn(command, args, { cwd, stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit', encoding: 'utf8' });
  if (result.error) fail(`${command} could not start: ${result.error.message}`);
  if (result.status !== 0) fail(`"${command} ${args.slice(0, 3).join(' ')}" exited with code ${result.status}`);
  return result.stdout ?? '';
}

/** Full output of `<command> <args>`, or null if the command is missing or fails. */
export function probeAll(command, args = ['--version']) {
  const result = spawn(command, args, { stdio: ['ignore', 'pipe', 'ignore'], encoding: 'utf8' });
  return result.status === 0 ? (result.stdout ?? '') : null;
}

/** First line of `<command> <args>`, or null if the command is missing or fails. */
export function probe(command, args = ['--version']) {
  const out = probeAll(command, args);
  return out === null ? null : (out.trim().split(/\r?\n/)[0] ?? '');
}

export function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

/** Rewrites `<bundleDir>/SHA256SUMS` for every file in `<bundleDir>/installers` (same format as the setup scripts). */
export function writeBundleChecksums(bundleDir) {
  const installers = join(bundleDir, 'installers');
  const lines = readdirSync(installers, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => entry.name)
    .sort()
    .map((name) => `${sha256File(join(installers, name))}  installers/${name}`);
  writeFileSync(join(bundleDir, 'SHA256SUMS'), `${lines.join('\n')}\n`);
}

export function readPackageVersion() {
  return JSON.parse(readFileSync(join(repoRoot, 'package.json'), 'utf8')).version;
}
