#!/usr/bin/env node
/**
 * bundle-check.mjs — gzip-size budget gate (grid #53).
 * BUDGET_MAIN_KB: main app chunk (index-*.js) gzip limit (default 200 KB).
 * BUDGET_TOTAL_KB: total initial JS gzip limit (default 500 KB).
 */
import { readFileSync, readdirSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

// fileURLToPath (not .pathname): URL pathnames keep a leading "/" before the
// drive letter on Windows, which breaks path resolution there.
const DIST = fileURLToPath(new URL('../dist/assets', import.meta.url));
const MAIN_BUDGET_KB = Number(process.env.BUDGET_MAIN_KB ?? 200);
const TOTAL_BUDGET_KB = Number(process.env.BUDGET_TOTAL_KB ?? 500);

const files = readdirSync(DIST).filter((f) => f.endsWith('.js'));
let mainGzipKb = 0;
let totalGzipKb = 0;

for (const f of files) {
  const buf = readFileSync(join(DIST, f));
  const gzipKb = gzipSync(buf).length / 1024;
  totalGzipKb += gzipKb;
  if (f.startsWith('index-')) mainGzipKb = gzipKb;
}

console.log(`bundle: main ${mainGzipKb.toFixed(1)} KB gzip (budget ${MAIN_BUDGET_KB} KB)`);
console.log(`bundle: total ${totalGzipKb.toFixed(1)} KB gzip (budget ${TOTAL_BUDGET_KB} KB)`);

let failed = false;
if (mainGzipKb > MAIN_BUDGET_KB) {
  console.error(`FAIL: main chunk exceeds budget`);
  failed = true;
}
if (totalGzipKb > TOTAL_BUDGET_KB) {
  console.error(`FAIL: total initial JS exceeds budget`);
  failed = true;
}
if (files.length === 0) {
  console.error('FAIL: no JS assets found — did `pnpm build` run?');
  failed = true;
}
process.exit(failed ? 1 : 0);
