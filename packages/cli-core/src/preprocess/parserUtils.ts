/**
 * Shared tokenisation helpers for the preprocess layer. Quote- and
 * paren-aware so callers can split user input without re-implementing
 * the same walks. Re-exported by `ifElse.ts` and `conditions.ts`
 * (substitute.ts still keeps a private copy for now — refactor later
 * if the two diverge in subtle ways).
 */

/**
 * Peel a parenthesised block from the start of `input`. Returns the
 * inner body (without the surrounding parens) and the number of
 * input chars consumed (including the parens). Quote- and
 * paren-balanced so nested parens or quoted parens don't confuse
 * the depth counter.
 *
 * Returns `null` if `input` doesn't start with `(` or if the parens
 * never balance.
 */
export function peelParenBlock(
  input: string,
): { body: string; consumed: number } | null {
  if (input[0] !== '(') return null;
  let depth = 0;
  let inQuotes: '"' | "'" | null = null;
  for (let i = 0; i < input.length; i++) {
    const c = input[i];
    if (inQuotes) {
      if (c === inQuotes && input[i - 1] !== '\\') inQuotes = null;
      continue;
    }
    if (c === '"' || c === "'") {
      inQuotes = c;
      continue;
    }
    if (c === '(') depth++;
    else if (c === ')') {
      depth--;
      if (depth === 0) {
        return { body: input.slice(1, i), consumed: i + 1 };
      }
    }
  }
  return null;
}

/**
 * Order ID / suffix shape used by `cancel <id>` and `wait fill <id>`.
 * 3+ chars of `[A-Za-z0-9_-]`. Short enough to allow suffix matching
 * (`abc` matches `order-xyz-abc`) while strict enough to rule out
 * obvious garbage. Exposed here so both consumers stay in sync — if
 * you tighten this, both `cancel` and `wait fill` get the new shape
 * simultaneously.
 */
export const ID_SUFFIX_RE = /^[A-Za-z0-9_-]{3,}$/;

/**
 * Whitespace tokeniser that leaves quote characters in place. Doesn't
 * strip quotes — downstream parsers either preserve them or strip
 * locally. Tabs and spaces are both separators.
 */
export function whitespaceTokenize(input: string): string[] {
  const out: string[] = [];
  let buf = '';
  let inQuotes: '"' | "'" | null = null;
  for (let i = 0; i < input.length; i++) {
    const c = input[i];
    if (inQuotes) {
      if (c === inQuotes && input[i - 1] !== '\\') inQuotes = null;
      buf += c;
      continue;
    }
    if (c === '"' || c === "'") {
      inQuotes = c;
      buf += c;
      continue;
    }
    if (c === ' ' || c === '\t') {
      if (buf) {
        out.push(buf);
        buf = '';
      }
      continue;
    }
    buf += c;
  }
  if (buf) out.push(buf);
  return out;
}
