import { substituteTokens, whitespaceTokenize } from './substitute';
import type { PreprocessContext } from './types';

/**
 * Reactive variables (`sets @<name> <source>`) re-evaluate their value on
 * each substitution rather than snapshotting at definition time.
 *
 * The SOURCE is any string the substitute layer can resolve to a single
 * numeric token — a `$constant` (`$last`, `$ask`, `$mark`, …), a math
 * expression (`$ask-$bid`, `$last*0.99`, `@mul*2`), a multi-token
 * price-anchor + offset (`$entry +1%`), or a bare numeric literal.
 *
 * Evaluation reuses the full substitute layer, so the source vocabulary
 * is automatically in sync with what `buy 1$ at <SOURCE>` would accept.
 * The catch is that any of the substitute layer's error paths (no
 * position for `$entry`, exchange not connected, etc.) become per-use
 * errors on `@<name>`. Each one is wrapped with the var name so the
 * user can trace it back to the `sets` that introduced it.
 *
 * Source storage is intentionally session-scoped — see
 * `PreprocessStore.setLiveVar`. Persisting a "live" string across
 * sessions would either:
 *   - re-subscribe on next launch (out of scope for v1; we'd need a
 *     subscription engine), or
 *   - persist a stale snapshot (defeats the purpose of `sets`).
 *
 * Static `set @x V` and reactive `sets @x SRC` are mutually exclusive
 * on the same name — re-binding via either path clears the other —
 * but the storage layer enforces that, not this module.
 */

/**
 * Evaluate a live-var source against the current context. Returns the
 * numeric value (as a number, not a string — caller decides format).
 *
 * Throws when:
 *   - source is empty / whitespace-only
 *   - substitute layer throws (unknown constant, no position, etc.)
 *   - substitute result is not a single token (source was multi-valued)
 *   - the result token can't be parsed as a finite number
 *   - the live var is part of a cycle (`sets @a @b; sets @b @a`)
 *
 * All thrown errors are wrapped with `live var @<name>:` prefix so the
 * caller doesn't have to re-wrap.
 *
 * `seen` is the recursion stack — when `@<name>` substitution inside a
 * source references another live var, that live var's name is pushed
 * before recursing. If the same name appears twice, we have a cycle.
 * Callers from outside the live-var module should omit `seen`; the
 * substitute layer threads it for nested calls.
 */
export function evaluateLiveVarSource(
  name: string,
  source: string,
  ctx: PreprocessContext,
  seen: Set<string> = new Set(),
): number {
  const trimmed = source.trim();
  if (!trimmed) {
    throw new Error(`live var @${name}: empty source`);
  }
  if (seen.has(name)) {
    throw new Error(
      `live var @${name}: cycle detected (chain: ${[...seen, name].map((n) => `@${n}`).join(' → ')})`,
    );
  }
  const nextSeen = new Set(seen);
  nextSeen.add(name);
  const tokens = whitespaceTokenize(trimmed);
  let substituted: string[];
  try {
    substituted = substituteTokens(tokens, ctx, nextSeen);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    // Don't re-wrap an already-wrapped live var error — preserves the
    // original `@a → @b → @a` cycle chain when nested.
    if (msg.startsWith('live var @')) throw err;
    throw new Error(`live var @${name} source '${source}': ${msg}`);
  }
  if (substituted.length !== 1) {
    throw new Error(
      `live var @${name} source '${source}': expected single value, got ${substituted.length} tokens`,
    );
  }
  const valueStr = substituted[0];
  const value = parseFloat(valueStr);
  if (!Number.isFinite(value)) {
    throw new Error(
      `live var @${name} source '${source}': result '${valueStr}' is not numeric`,
    );
  }
  return value;
}

/**
 * Cheap shape-check used by `sets @<name> <source>` to reject obvious
 * garbage at definition time. Does NOT evaluate — semantic errors
 * (unknown constant, missing position, …) still surface at first use.
 *
 * Accepts source if it's non-empty and every whitespace-separated token
 * looks like one of:
 *   - a numeric literal, with optional sign and percent suffix
 *     (covers `50`, `-2.5`, `+1%`, `-1.1%`)
 *   - a `$<name>` constant (UI-only constants included — those error
 *     at evaluation, not here)
 *   - a `@<name>` user var
 *   - a math token (contains an operator between two operand-shaped
 *     chunks — same recogniser the substitute layer uses)
 *   - a sized literal `<N>$` / `$<N>` — the substitute layer leaves
 *     these alone so they reach commands intact, but they don't
 *     evaluate to a single number on their own, so we still reject
 *     them as sources here.
 *
 * Returns null on accept; a string error message on reject. Caller
 * surfaces the message in the dispatcher's error frame.
 */
export function validateLiveVarSource(source: string): string | null {
  const trimmed = source.trim();
  if (!trimmed) return 'empty source';
  const tokens = whitespaceTokenize(trimmed);
  for (const tok of tokens) {
    if (!isSourceTokenShape(tok)) {
      return `unrecognised source token '${tok}'`;
    }
  }
  return null;
}

const NUMERIC_LITERAL_RE = /^[+-]?(?:\d+(?:\.\d+)?|\.\d+)%?$/;
const DOLLAR_NAME_RE = /^\$[a-zA-Z][\w]*$/;
const AT_NAME_RE = /^@[a-zA-Z][\w]*$/;
// Same shape MATH_TOKEN_RE uses in math.ts. Duplicated here to avoid
// an import cycle and because the math module deliberately keeps its
// regex private. If we ever need to share, refactor it into a shared
// `tokenShapes.ts`.
const MATH_TOKEN_SHAPE_RE = /^(-?(?:\d+(?:\.\d+)?|\.\d+|\$[a-zA-Z][\w]*|@[a-zA-Z][\w]*))(?:[+\-*/](?:\d+(?:\.\d+)?|\.\d+|\$[a-zA-Z][\w]*|@[a-zA-Z][\w]*))+$/;

function isSourceTokenShape(tok: string): boolean {
  return (
    NUMERIC_LITERAL_RE.test(tok) ||
    DOLLAR_NAME_RE.test(tok) ||
    AT_NAME_RE.test(tok) ||
    MATH_TOKEN_SHAPE_RE.test(tok)
  );
}
