/** Type surface for scripts/i18n-sweep.mjs (consumed by the sweep test). */
export interface SweepResult {
  defined: string[];
  used: string[];
  unused: string[];
  missing: string[];
}
export function definedKeys(enSource: string): Set<string>;
/** Paren-balanced extraction of every `t(…)` call's argument text. */
export function tCalls(source: string): string[];
/** Dotted key literals inside a t() call's first argument. */
export function firstArgKeyLiterals(call: string): string[];
export function analyze(): SweepResult;
