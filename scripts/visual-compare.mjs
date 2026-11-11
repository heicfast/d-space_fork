#!/usr/bin/env node
/**
 * Visual comparison verifier — checks captured Playwright step screenshots
 * in `test-results/smoke-step-*.png` against the corpus viewport and PNG
 * header geometry specification (`docs/visual-spec.md`).
 *
 * Invoked by `.github/workflows/visual-compare.yml` (`pnpm visual:compare`).
 */
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const TEST_RESULTS_DIR = 'test-results';
const EXPECTED_STEPS = [
  '05-scan-live.png',
  '10-scan-complete.png',
  '20-results.png',
  '30-filter-open.png',
  '40-preview.png',
  '50-report.png',
  '60-sidebar-collapsed.png',
];

function readPngDimensions(buf) {
  // PNG signature (8 bytes) + IHDR length (4 bytes) + 'IHDR' (4 bytes) + width (4B BE) + height (4B BE)
  if (buf.length < 24 || buf.toString('ascii', 1, 4) !== 'PNG') {
    throw new Error('Invalid PNG header');
  }
  const width = buf.readUInt32BE(16);
  const height = buf.readUInt32BE(20);
  return { width, height };
}

function main() {
  if (!existsSync(TEST_RESULTS_DIR)) {
    console.error(
      `visual-compare: directory '${TEST_RESULTS_DIR}' does not exist. Run 'pnpm e2e' first.`,
    );
    process.exit(1);
  }

  const files = readdirSync(TEST_RESULTS_DIR);
  const rows = [];
  let failed = false;

  for (const stepFile of EXPECTED_STEPS) {
    const fullPath = join(TEST_RESULTS_DIR, stepFile);
    if (!files.includes(stepFile)) {
      rows.push({ stepFile, status: 'MISSING', width: 0, height: 0, bytes: 0 });
      failed = true;
      continue;
    }
    const buf = readFileSync(fullPath);
    const { width, height } = readPngDimensions(buf);
    const validDims = width >= 1018 && height >= 716 && buf.length > 10_000;
    if (!validDims) failed = true;
    rows.push({
      stepFile,
      status: validDims ? 'PASS' : 'FAIL',
      width,
      height,
      bytes: buf.length,
    });
  }

  const summary = {
    generatedAt: new Date().toISOString(),
    expectedSteps: EXPECTED_STEPS.length,
    passedSteps: rows.filter((r) => r.status === 'PASS').length,
    rows,
  };

  writeFileSync(
    join(TEST_RESULTS_DIR, 'visual-compare-summary.json'),
    JSON.stringify(summary, null, 2),
  );

  for (const row of rows) {
    console.log(
      `[visual-compare] ${row.status.padEnd(7)} ${row.stepFile} (${row.width}x${row.height}, ${row.bytes} bytes)`,
    );
  }

  if (failed) {
    console.error('visual-compare: one or more step screenshots failed verification.');
    process.exit(1);
  }
  console.log('visual-compare: all step screenshots verified against corpus viewport spec.');
}

main();
