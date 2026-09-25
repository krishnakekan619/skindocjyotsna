#!/usr/bin/env node
// Verifies that every dependency is pinned to a STABLE release with a checksum, so every
// build is traceable:
//
//   package.json       direct npm dependencies use exact versions (no ^ ~ ranges)
//   package-lock.json  every npm package: stable version, https registry URL, sha512 integrity
//   Cargo.lock         every crates.io package: stable version and checksum
//   tools.lock         (Windows + macOS setup bundles) every installer: stable version + SHA-256
//
//   node scripts/check-locks.mjs            warnings for locks that do not exist yet
//   node scripts/check-locks.mjs --strict   missing Cargo.lock / tools.lock is an error (release builds)

import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { repoRoot } from './lib/run.mjs';

const strict = process.argv.includes('--strict');
const errors = [];
const warnings = [];
const PRERELEASE = /^\d+\.\d+\.\d+-/; // semver pre-release: 1.2.3-rc.1, 2.0.0-beta ...

function missing(what) {
  (strict ? errors : warnings).push(`${what} does not exist yet`);
}

function checkPackageJson() {
  const pkg = JSON.parse(readFileSync(join(repoRoot, 'package.json'), 'utf8'));
  for (const field of ['dependencies', 'devDependencies']) {
    for (const [name, spec] of Object.entries(pkg[field] ?? {})) {
      if (!/^\d+\.\d+\.\d+$/.test(spec)) errors.push(`package.json ${field}.${name} = "${spec}" is not an exact stable version`);
    }
  }
}

function checkPackageLock() {
  const path = join(repoRoot, 'package-lock.json');
  if (!existsSync(path)) return missing('package-lock.json');
  const lock = JSON.parse(readFileSync(path, 'utf8'));
  if ((lock.lockfileVersion ?? 0) < 3) errors.push(`package-lock.json lockfileVersion ${lock.lockfileVersion} < 3`);
  let count = 0;
  for (const [key, entry] of Object.entries(lock.packages ?? {})) {
    if (key === '' || entry.link) continue;
    count++;
    const name = key.replace(/^.*node_modules\//, '');
    if (PRERELEASE.test(entry.version ?? '')) errors.push(`npm ${name}@${entry.version} is a pre-release`);
    if (!entry.resolved?.startsWith('https://registry.npmjs.org/')) errors.push(`npm ${name}: unexpected source ${entry.resolved}`);
    if (!entry.integrity?.startsWith('sha512-')) errors.push(`npm ${name}: missing sha512 integrity`);
  }
  return count;
}

function checkCargoLock() {
  const path = join(repoRoot, 'Cargo.lock');
  if (!existsSync(path)) return missing('Cargo.lock (created by the first cargo build)');
  let count = 0;
  for (const block of readFileSync(path, 'utf8').split('[[package]]').slice(1)) {
    const field = (key) => block.match(new RegExp(`^${key} = "([^"]*)"`, 'm'))?.[1];
    const [name, version, source] = [field('name'), field('version'), field('source')];
    if (!source) continue; // our own workspace crates
    count++;
    if (!source.startsWith('registry+https://github.com/rust-lang/crates.io-index')) errors.push(`crate ${name}: unexpected source ${source}`);
    if (PRERELEASE.test(version ?? '')) errors.push(`crate ${name}@${version} is a pre-release`);
    if (!field('checksum')) errors.push(`crate ${name}@${version}: missing checksum`);
  }
  return count;
}

function checkToolsLock(os) {
  const path = join(repoRoot, 'setup', os, 'tools.lock');
  if (!existsSync(path)) return missing(`setup/${os}/tools.lock (created by the first Prepare run)`);
  let count = 0;
  for (const line of readFileSync(path, 'utf8').split(/\r?\n/)) {
    const m = line.match(/^([A-Z0-9_]+)=(.*)$/);
    if (!m) continue;
    const [, key, value] = m;
    if (key.endsWith('_SHA256')) {
      if (key === 'CLT_SHA256' && value === '') continue; // Xcode CLT .dmg is optional
      count++;
      if (!/^[0-9a-f]{64}$/.test(value)) errors.push(`setup/${os}/tools.lock ${key} is not a SHA-256`);
    }
    if (key.endsWith('_VERSION') && PRERELEASE.test(value)) errors.push(`setup/${os}/tools.lock ${key}=${value} is a pre-release`);
  }
  return count;
}

checkPackageJson();
const npm = checkPackageLock();
const crates = checkCargoLock();
const winTools = checkToolsLock('windows');
const macTools = checkToolsLock('macos');

console.log('Lock check:');
console.log(`  npm packages       ${npm ?? '-'}`);
console.log(`  Rust crates        ${crates ?? '-'}`);
console.log(`  Windows installers ${winTools ?? '-'}`);
console.log(`  macOS installers   ${macTools ?? '-'}`);
for (const w of warnings) console.warn(`[WARN] ${w}`);
for (const e of errors) console.error(`[FAIL] ${e}`);
if (errors.length) process.exit(1);
console.log(`[OK]   All present locks pin stable releases with checksums${warnings.length ? ' (see warnings)' : ''}.`);
