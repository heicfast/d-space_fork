#!/usr/bin/env node
/**
 * arch-lint.mjs — architecture boundary enforcement (belt and braces
 * alongside eslint import-x/no-restricted-paths and ast-grep rules).
 *
 * Rules (see docs/ARCHITECTURE.md):
 *  A1. Only src/queries/** and src/connector/** may import the connector
 *      FACTORY or its implementations (`@/connector`, `@/connector/mock/*`,
 *      `@/connector/tauri/*`). TYPE-ONLY imports of `@/connector/contract`
 *      are legal everywhere — the contract is the shared vocabulary.
 *  A2. No `invoke(` outside src/connector/**.
 *  A3. src/components/** must not import from src/features/**.
 *  A4. src/app/** must not import from src/features/** except the shell/router
 *      wiring (which is exactly where composition happens).
 *  A5. Feature folders must not import each other, with the documented
 *      compositions: scan-progress → results-explorer (scan renders inside
 *      the results surface) and results-explorer → preview (the preview pane
 *      is the right-hand pane of the explorer).
 *  A6. No `any` in exported types of connector contract files.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

// fileURLToPath (not .pathname): URL pathnames keep a leading "/" before the
// drive letter on Windows, which path.join then resolves as a drive-absolute
// path — producing the phantom "D:\D:\a\..." root.
const ROOT = fileURLToPath(new URL('..', import.meta.url));
const SRC = join(ROOT, 'src');

const violations = [];
const files = [];

function walk(dir) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      walk(full);
    } else if (/\.(ts|tsx)$/.test(entry)) {
      files.push(full);
    }
  }
}
walk(SRC);

const rel = (f) => relative(ROOT, f).replaceAll('\\', '/');

for (const file of files) {
  const path = rel(file);
  const text = readFileSync(file, 'utf8');

  // A1: connector factory/implementation import boundary (type imports of
  // the contract are intentionally allowed everywhere)
  const factoryImport =
    /from\s+['"]@\/connector['"]/.test(text) ||
    /from\s+['"]@\/connector\/(mock|tauri)\//.test(text) ||
    /from\s+['"]\.\.\/\.\.\/(connector)['"]/.test(text);
  if (factoryImport && !path.startsWith('src/queries/') && !path.startsWith('src/connector/')) {
    violations.push(
      `A1 ${path}: imports the connector factory/implementation outside queries/`,
    );
  }

  // A2: raw tauri invoke
  if (/\binvoke\s*\(/.test(text) && !path.startsWith('src/connector/')) {
    violations.push(`A2 ${path}: calls invoke() outside the connector layer`);
  }

  // A3: components must not reach into features
  if (path.startsWith('src/components/') && /from\s+['"]@\/features\//.test(text)) {
    violations.push(`A3 ${path}: design-system component imports a feature`);
  }

  // A4: app layer may only import features in the shell/router wiring
  if (
    path.startsWith('src/app/') &&
    !['src/app/shell.tsx', 'src/app/router.tsx'].includes(path)
  ) {
    if (/from\s+['"]@\/features\//.test(text)) {
      violations.push(`A4 ${path}: non-shell app module imports a feature`);
    }
  }

  // A5: cross-feature imports (documented compositions exempt)
  const featureMatch = path.match(/^src\/features\/([^/]+)\//);
  if (featureMatch) {
    const own = featureMatch[1];
    const ALLOWED = new Map([
      ['scan-progress', ['results-explorer']],
      ['results-explorer', ['preview']],
    ]);
    const crossImports = [...text.matchAll(/from\s+['"]@\/features\/([^/'"]+)/g)].map(
      (m) => m[1],
    );
    for (const imported of crossImports) {
      if (imported !== own && !(ALLOWED.get(own) ?? []).includes(imported)) {
        violations.push(`A5 ${path}: imports feature '${imported}' (own: '${own}')`);
      }
    }
  }

  // A6: no `any` in exported types of connector contract files (docs/ARCHITECTURE.md)
  if (path.startsWith('src/connector/')) {
    const exportedAny = /export\s+(?:type|interface)\s+\w+[^=]*=\s*[^;\n]*\bany\b/.test(text);
    const anyProp = /export\s+(?:type|interface)\b[\s\S]*?:\s*any\b/.test(
      text.match(/export\s+(?:type|interface)\b[\s\S]*?(?=\nexport|\n*$)/g)?.join('\n') ?? '',
    );
    if (exportedAny || anyProp) {
      violations.push(`A6 ${path}: exported type uses \`any\``);
    }
  }
}

if (violations.length > 0) {
  console.error(`arch-lint: ${violations.length} violation(s)`);
  for (const v of violations) console.error(`  ${v}`);
  process.exit(1);
}
console.log(`arch-lint: OK (${files.length} files checked)`);
