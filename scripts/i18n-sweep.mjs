#!/usr/bin/env node
/**
 * i18n-sweep.mjs — i18n surface analysis (T12 governance).
 *
 * analyze() reports:
 *  - defined: keys in src/i18n/en.ts
 *  - referenced: keys referenced from src (t('…') static, ternary-literal keys
 *    inside t() calls — paren-balanced, so nested calls don't hide them —
 *    t(`…${…}`) template families expanded against their real source domains
 *    (contract type unions + filter CATEGORIES), labelKey/reasonKey/
 *    descriptionKey indirection, connector reason-literal families)
 *  - unused: defined but never referenced (dead surface — must be 0)
 *  - missing: referenced but not defined (raw-key render bugs — must be 0)
 *
 * Enforced by tests/i18n-sweep.test.ts; also runnable as a CLI report.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const ROOT = join(import.meta.dirname, '..');
const SRC = join(ROOT, 'src');

function walk(dir, out = []) {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(tsx?|css|html)$/.test(f)) out.push(p);
  }
  return out;
}

/** Extract the flattened key set from the en.ts object literal. */
export function definedKeys(enSource) {
  const defined = new Set();
  const ns = [];
  let buf = '';
  let idx = 0;
  const line = enSource;
  while (idx < line.length) {
    const ch = line[idx];
    if (ch === '{') {
      ns.push(buf);
      buf = '';
      idx++;
      continue;
    }
    if (ch === '}') {
      ns.pop();
      idx++;
      continue;
    }
    const m = /^([A-Za-z0-9_]+)\s*:\s*/.exec(line.slice(idx));
    if (m) {
      const key = m[1];
      idx += m[0].length;
      const rest = line.slice(idx);
      if (rest.startsWith('{')) {
        ns.push(key);
        idx++;
      } else {
        defined.add([...ns, key].filter(Boolean).join('.'));
        const twin = /^([A-Za-z0-9_]+)_other\s*:/.exec(rest);
        if (twin) defined.add([...ns, `${key}_other`].filter(Boolean).join('.'));
        const v = /^(['"`])((?:\\.|(?!\1).)*)\1\s*,/.exec(rest);
        idx += v ? v[0].length : 0;
      }
      continue;
    }
    idx++;
  }
  return defined;
}

/**
 * Collect the full argument text of every `t(…)` call, balancing parens and
 * skipping string/template bodies — regex-only extraction misses ternary keys
 * once the call contains nested parens (e.g. t(cond ? 'a' : 'b', f(x)));
 * that exact blind spot purged two live keys once already.
 */
export function tCalls(source) {
  const calls = [];
  for (let i = 0; i < source.length - 1; i++) {
    if (source.slice(i, i + 2) !== 't(') continue;
    if (i > 0 && /[\w$]/.test(source[i - 1])) continue; // not a standalone t(
    let depth = 1;
    let j = i + 2;
    while (j < source.length && depth > 0) {
      const ch = source[j];
      if (ch === "'" || ch === '"' || ch === '`') {
        const q = ch;
        j++;
        while (j < source.length && source[j] !== q) {
          if (source[j] === '\\') j++;
          j++;
        }
      } else if (ch === '(') depth++;
      else if (ch === ')') depth--;
      j++;
    }
    calls.push(source.slice(i + 2, j - 1));
  }
  return calls;
}

/** Key literals inside a t() call's first argument (up to a top-level comma). */
export function firstArgKeyLiterals(call) {
  let depth = 0;
  let arg = '';
  for (let i = 0; i < call.length; i++) {
    const ch = call[i];
    if (ch === "'" || ch === '"' || ch === '`') {
      const q = ch;
      arg += ch;
      i++;
      while (i < call.length && call[i] !== q) {
        arg += call[i] ?? '';
        if (call[i] === '\\') {
          i++;
          arg += call[i] ?? '';
        }
        i++;
      }
      arg += q;
      continue;
    }
    if (ch === '(' || ch === '[' || ch === '{') depth++;
    if (ch === ')' || ch === ']' || ch === '}') depth--;
    if (ch === ',' && depth === 0) break;
    arg += ch;
  }
  return [...arg.matchAll(/'([A-Za-z][\w]*(?:\.[\w]+)+)'/g)].map((m) => m[1]);
}

/** String-literal members of a TS union type in `source`. */
function unionLiterals(source, typeName) {
  const m = new RegExp(
    `export type ${typeName} = ([^;]+);|export type ${typeName}\\n?\\s*=\\n?\\s*([\\s\\S]*?);`,
  ).exec(source);
  const body = m ? (m[1] ?? m[2] ?? '') : '';
  return [...body.matchAll(/'([a-zA-Z-]+)'/g)].map((x) => x[1]);
}

export function analyze() {
  const files = walk(SRC);
  const corpus = files.map((p) => ({ p, s: readFileSync(p, 'utf8') }));
  const defined = definedKeys(readFileSync(join(SRC, 'i18n/en.ts'), 'utf8'));

  const used = new Set();
  const DYNAMIC = new Map(); // prefix -> resolved domain keys
  for (const { p, s } of corpus) {
    if (p.endsWith('.html') || p.endsWith('.css')) continue;
    // Every t() call — paren-balanced first argument, all key literals inside
    // (plain calls, ternary chains, nested expressions).
    for (const call of tCalls(s)) {
      for (const k of firstArgKeyLiterals(call)) used.add(k);
      const tmpl = /^`([A-Za-z][\w]*(?:\.[\w]+)*\.?)\$\{/.exec(call.trim());
      if (tmpl) DYNAMIC.set(tmpl[1], tmpl[1]);
    }
    for (const m of s.matchAll(/\bt\(\s*`([^`$]*)`/g)) used.add(m[1]);
    for (const m of s.matchAll(/labelKey:\s*'([^']+)'/g)) used.add(m[1]);
    for (const m of s.matchAll(/reasonKey[:=]\s*'([^']+)'/g)) used.add(m[1]);
    for (const m of s.matchAll(/finding:\s*'([^']+)'/g)) used.add(m[1]);
    for (const m of s.matchAll(/descriptionKey:\s*'([^']+)'/g)) used.add(m[1]);
    // THEME_LABEL_KEY-style key maps: Record<Mode, 'settings.theme…'> values.
    for (const m of s.matchAll(/'(settings\.theme\w+)'\s*[,}]/g)) used.add(m[1]);
  }
  // Connector reason/finding literals (connError positional args, mock
  // descriptionKey values) and any other key-shaped literals in the codebase.
  for (const { s } of corpus) {
    for (const m of s.matchAll(
      /'((?:errors|preview\.reason|recovery|repair|about)\.[A-Za-z0-9_.]+)'/g,
    ))
      used.add(m[1]);
  }
  // Ternary/conditional keyed calls — t(cond ? 'a.b' : 'c.d') — and any other
  // reference a key-shaped literal ANYWHERE in source. Over-approximation is
  // deliberate: a false "used" only weakens the dead-key check, while a false
  // "unused" deletes a live key and renders raw keys at runtime (this exact
  // regression shipped scan.quickScanning once — the e2e caught it on
  // Windows; the unit suite could not).
  const NAMESPACES =
    'app|about|common|nav|sidebar|home|categories|scan|results|preview|recovery|health|repair|settings|errors|a11y|devPanel|lostPartition';
  for (const { s } of corpus) {
    for (const m of s.matchAll(new RegExp(`'((${NAMESPACES})\\.[A-Za-z0-9_.]+)'`, 'g')))
      used.add(m[1]);
  }

  // Template families: resolve each prefix against its REAL domain (the
  // contract type unions that produce the variable's values) so a missing
  // family member is caught, not silently skipped. Unknown prefixes that
  // resolve to zero defined keys are reported as missing families — a
  // template the catalogs cannot ever satisfy is a raw-key render bug.
  const contract = readFileSync(join(SRC, 'connector/contract.ts'), 'utf8');
  const gradeDomain = unionLiterals(contract, 'ConfidenceGrade');
  const collisionDomain = unionLiterals(contract, 'CollisionPolicy');
  const catDomain = unionLiterals(contract, 'FileCategoryId');
  for (const prefix of DYNAMIC.keys()) {
    let domain = null;
    if (prefix === 'results.confidence.') domain = gradeDomain;
    else if (prefix === 'recovery.collision') domain = collisionDomain.map(titleCase);
    else if (prefix === 'categories.') domain = catDomain;
    if (domain) {
      for (const d of domain) used.add(`${prefix}${d}`);
    } else {
      // Unknown family: expand against defined (the historical behavior) …
      const matches = [...defined].filter((k) => k.startsWith(prefix));
      if (matches.length === 0)
        used.add(`${prefix}⟨family⟩`); // …and flag it
      else for (const k of matches) used.add(k);
    }
  }
  // i18next plural pairs: referencing `X` with a count also resolves `X_other`
  // (and vice versa) — one entry, two keys. Treat pairs as used together.
  const usedWithTwins = new Set(used);
  for (const u of used) {
    if (u.endsWith('_other')) usedWithTwins.add(u.slice(0, -6));
    else usedWithTwins.add(`${u}_other`);
  }

  const unused = [...defined].filter((k) => !usedWithTwins.has(k)).sort();
  const missing = [...used].filter((k) => !defined.has(k)).sort();
  return { defined: [...defined].sort(), used: [...used].sort(), unused, missing };
}

function titleCase(s) {
  return s[0].toUpperCase() + s.slice(1);
}

// CLI report when run directly.
const invokedDirectly =
  typeof process !== 'undefined' &&
  process.argv[1] &&
  import.meta.url === `file://${process.argv[1]}`;
if (invokedDirectly) {
  const { defined, used, unused, missing } = analyze();
  console.log(`defined=${defined.length} referenced=${used.length}`);
  console.log(`\n== UNUSED (${unused.length}) ==`);
  for (const k of unused) console.log(k);
  console.log(`\n== REFERENCED-BUT-MISSING (${missing.length}) ==`);
  for (const k of missing) console.log(k);
}
