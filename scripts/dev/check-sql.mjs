#!/usr/bin/env node
// Development check: prepares every SQL statement found in the Rust repositories against the
// real schema (all migrations), using Node's built-in SQLite. Catches unknown columns/tables and
// syntax errors without compiling Rust.
//
//   node scripts/dev/check-sql.mjs
import { DatabaseSync } from 'node:sqlite';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const migrations = join(root, 'crates', 'clinic-sqlite', 'migrations');
const db = new DatabaseSync(':memory:');
for (const file of readdirSync(migrations).sort()) db.exec(readFileSync(join(migrations, file), 'utf8'));
// Tables that exist only inside backup files (backup_meta) or a test (product_search).
db.exec(`CREATE TABLE backup_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL) STRICT;
         CREATE VIRTUAL TABLE product_search USING fts5(name, generic_name);`);

const dirs = [join(root, 'crates', 'clinic-sqlite', 'src', 'repo'), join(root, 'crates', 'clinic-sqlite', 'src')];
const files = dirs.flatMap((dir) => readdirSync(dir).filter((f) => f.endsWith('.rs')).map((f) => join(dir, f)));

const STRING_LITERAL = /"(?:[^"\\]|\\[\s\S])*"/g;
const CONST_STRING = /const (\w+): &str = ("(?:[^"\\]|\\[\s\S])*");/g;
// Rust string contents: drop line continuations (backslash + newline + indentation), unescape quotes.
const unquote = (literal) => literal.slice(1, -1).replace(/\\\r?\n\s*/g, '').replace(/\\"/g, '"');

let checked = 0;
const failures = [];
for (const file of files) {
  const text = readFileSync(file, 'utf8');
  const consts = Object.fromEntries([...text.matchAll(CONST_STRING)].map((m) => [m[1], unquote(m[2])]));
  for (const match of text.matchAll(STRING_LITERAL)) {
    const sql = unquote(match[0]).replace(/\{(\w+)\}/g, (all, name) => consts[name] ?? all).trim();
    if (!/^(SELECT|INSERT|UPDATE|DELETE|WITH)\s/i.test(sql) || /\{\w+\}/.test(sql)) continue;
    checked++;
    try {
      db.prepare(sql);
    } catch (error) {
      failures.push(`${file.split(/[\\/]/).slice(-2).join('/')}: ${error.message}\n       ${sql.replace(/\s+/g, ' ').slice(0, 180)}`);
    }
  }
}
console.log(`SQL statements prepared against the schema: ${checked}`);
for (const failure of failures) console.log(`[FAIL] ${failure}`);
process.exit(failures.length ? 1 : 0);
