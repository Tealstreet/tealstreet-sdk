/**
 * Real script-args tokenizer per SPEC §7.1. Stateful single-pass over the raw
 * command line. Three jobs:
 *
 *   1. Tokenize honoring `"..."` / `'...'` (backslash-escapes inside quotes).
 *   2. Strip `--flag value` / `--flag=value` / `--bool` into a flags map;
 *      everything else is a positional.
 *   3. Build `symbols` from `positional[0]` split on comma + trimmed.
 *
 * Numeric tokens like `$100`, `100$`, `50%`, `50%bal`, `1%`, `-100` are kept
 * raw — scripts parse them themselves. The leading-dash rule:
 *   - `-<digit>` → literal (negative number)
 *   - `--<anything>` → long-flag
 *   - `-<letter>` (single short flag, no value) → boolean flag
 *   - bare `-` → literal
 *
 * The function is intentionally pure: no host state, no captured closures,
 * no side effects. Same call → same `ScriptArgs`. Tested at length in
 * `argParser.test.ts`.
 */
import type { ScriptArgs } from '../api/args.types';

const isFlagPrefix = (token: string): boolean => {
  if (!token.startsWith('-')) return false;
  // Negative numbers (e.g. `-100`, `-1.5`) are literals, not flags.
  if (/^-\d/.test(token)) return false;
  // Bare `-` is a literal (e.g. stdin sentinel) — not a flag.
  if (token === '-') return false;
  return true;
};

const splitFlagValue = (token: string): { name: string; value: string | undefined } => {
  // `--flag=value` → { name: 'flag', value: 'value' }; `--flag` → { name, undefined }.
  const stripped = token.replace(/^-+/, '');
  const eq = stripped.indexOf('=');
  if (eq === -1) return { name: stripped, value: undefined };
  return { name: stripped.slice(0, eq), value: stripped.slice(eq + 1) };
};

/** Tokenize honoring quotes + backslash-escapes. Whitespace splits otherwise. */
const tokenize = (input: string): string[] => {
  const out: string[] = [];
  let buf = '';
  let quote: '"' | "'" | null = null;
  let i = 0;
  while (i < input.length) {
    const ch = input[i];
    if (quote) {
      if (ch === '\\' && i + 1 < input.length) {
        buf += input[i + 1];
        i += 2;
        continue;
      }
      if (ch === quote) {
        quote = null;
        i++;
        continue;
      }
      buf += ch;
      i++;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      i++;
      continue;
    }
    if (ch === ' ' || ch === '\t' || ch === '\n') {
      if (buf.length > 0) {
        out.push(buf);
        buf = '';
      }
      i++;
      continue;
    }
    buf += ch;
    i++;
  }
  if (buf.length > 0) out.push(buf);
  return out;
};

export const parseScriptArgs = (raw: string): ScriptArgs => {
  const tokens = tokenize(raw);
  const positional: string[] = [];
  const flags: Record<string, string | boolean> = {};

  for (let i = 0; i < tokens.length; i++) {
    const tok = tokens[i];
    if (isFlagPrefix(tok)) {
      const { name, value } = splitFlagValue(tok);
      if (value !== undefined) {
        // `--flag=value`
        flags[name] = value;
        continue;
      }
      // `--flag VALUE` (if the next token isn't itself a flag/missing) vs bare boolean.
      const next = tokens[i + 1];
      if (next !== undefined && !isFlagPrefix(next)) {
        flags[name] = next;
        i++;
      } else {
        flags[name] = true;
      }
      continue;
    }
    positional.push(tok);
  }

  const first = positional[0];
  const symbols = first
    ? first
        .split(',')
        .map((s) => s.trim())
        .filter((s) => s.length > 0)
    : [];

  return Object.freeze({
    positional: Object.freeze(positional.slice()),
    flags: Object.freeze({ ...flags }),
    symbols: Object.freeze(symbols),
    raw,
  }) as ScriptArgs;
};
