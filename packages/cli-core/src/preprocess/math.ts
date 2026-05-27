import { evaluateLiveVarSource } from './reactiveVars';
import { resolveTrackedVar } from './trackedVars';
import type { PreprocessContext } from './types';

/**
 * Within-token arithmetic — `1*@qtymul`, `$entry-100`, `$ask-$bid`,
 * etc. Evaluated at substitution time so the downstream command
 * parsers always see a single numeric token.
 *
 * Scope (intentionally narrow):
 *   - Operands: float literals (`1`, `1.5`, `.5`), `$<name>` constants
 *     (resolved via `resolveOperand`), or `@<name>` user vars.
 *   - Operators: `+`, `-`, `*`, `/`.
 *   - Standard precedence: `*` and `/` bind tighter than `+` / `-`.
 *   - Left-to-right within a precedence level.
 *   - No parens, no exponent, no functions, no relative-percent
 *     (`50%@var`). Add later if a real use case appears.
 *
 * Detection is deliberately strict: only tokens that match
 * MATH_TOKEN_RE are even attempted. Anything else falls through to
 * the single-token resolver. This keeps `$entry` (no operator) and
 * `BTC/USDT` (slash inside a known symbol style) from being treated
 * as math.
 */

// Match a token shaped like "operand op operand (op operand)*" where
// `operand` is a float literal, `$ident`, or `@ident`. The leading
// operand may carry a unary minus so `-5*@x` works. Operators must
// have an operand on both sides (no trailing op, no double op).
const MATH_TOKEN_RE = /^(-?(?:[\d]+(?:\.[\d]+)?|\.[\d]+|\$[a-zA-Z][\w]*|@[a-zA-Z][\w]*))((?:[+\-*/](?:[\d]+(?:\.[\d]+)?|\.[\d]+|\$[a-zA-Z][\w]*|@[a-zA-Z][\w]*))+)$/;

// Tokenize the operator-chain tail. Each match is one [op, operand]
// pair. Operand pattern matches MATH_TOKEN_RE's operand alternatives.
const OP_OPERAND_RE = /([+\-*/])((?:[\d]+(?:\.[\d]+)?|\.[\d]+|\$[a-zA-Z][\w]*|@[a-zA-Z][\w]*))/g;

export type MathOperandResolver = (operand: string) => number;

/**
 * Try to evaluate `tok` as a math expression. Returns the numeric
 * result (as a string) on success, `null` if the token isn't a math
 * shape, and throws on any unresolved operand or division-by-zero.
 *
 * The caller passes `resolveOperand` so this module stays unaware of
 * the constants list (`$entry` / `$bid` / `$pnl` / …) and the var
 * store. Substitute layer wires the actual resolver.
 */
export function tryEvalMathToken(
  tok: string,
  resolveOperand: MathOperandResolver,
): string | null {
  const m = tok.match(MATH_TOKEN_RE);
  if (!m) return null;

  const firstOperand = m[1];
  const tail = m[2];

  // Build flat list of [operator, operand] pairs after the first.
  type Pair = { op: '+' | '-' | '*' | '/'; operand: string };
  const pairs: Pair[] = [];
  // Reset regex lastIndex defensively.
  OP_OPERAND_RE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = OP_OPERAND_RE.exec(tail)) !== null) {
    pairs.push({ op: match[1] as Pair['op'], operand: match[2] });
  }
  if (pairs.length === 0) {
    // The MATH_TOKEN_RE guarantees at least one trailing op+operand,
    // so this is just a defensive bail.
    return null;
  }

  // Resolve operands left-to-right, then evaluate with precedence:
  //   pass 1: collapse * and / left-to-right
  //   pass 2: collapse + and - left-to-right
  // Standard precedence, no parens.
  type Term = { value: number; opAfter: '+' | '-' | '*' | '/' | null };
  const terms: Term[] = [];
  terms.push({
    value: resolveOperand(firstOperand),
    opAfter: pairs.length > 0 ? pairs[0].op : null,
  });
  for (let i = 0; i < pairs.length; i++) {
    const next = pairs[i + 1];
    terms.push({
      value: resolveOperand(pairs[i].operand),
      opAfter: next ? next.op : null,
    });
  }

  // Pass 1 — fold * and /.
  for (let i = 0; i < terms.length - 1; ) {
    const op = terms[i].opAfter;
    if (op === '*' || op === '/') {
      const left = terms[i].value;
      const right = terms[i + 1].value;
      let combined: number;
      if (op === '*') {
        combined = left * right;
      } else {
        if (right === 0) {
          throw new Error(`math: division by zero in '${tok}'`);
        }
        combined = left / right;
      }
      terms[i] = { value: combined, opAfter: terms[i + 1].opAfter };
      terms.splice(i + 1, 1);
      // don't increment — re-check this position against the new neighbour
    } else {
      i++;
    }
  }

  // Pass 2 — fold + and -.
  let acc = terms[0].value;
  for (let i = 0; i < terms.length - 1; i++) {
    const op = terms[i].opAfter;
    const right = terms[i + 1].value;
    if (op === '+') acc = acc + right;
    else if (op === '-') acc = acc - right;
    else {
      // Shouldn't happen — pass 1 collapsed all */; defensive.
      throw new Error(`math: internal — leftover operator '${op}' in '${tok}'`);
    }
  }
  return formatMathResult(acc);
}

/**
 * Format a number for emission back into the token stream. We strip
 * trailing zeros so `2 * 0.5 = 1` rather than `1.0`, and clamp very
 * small floats to avoid `1.0000000000000002` style noise from ops
 * that should be exact.
 */
function formatMathResult(n: number): string {
  if (!Number.isFinite(n)) {
    throw new Error(`math: result is not finite (${n})`);
  }
  // Round to 12 sig digits to absorb IEEE-754 wobble while keeping
  // user-visible precision generous. Then strip trailing zeros.
  const fixed = Number(n.toPrecision(12));
  return String(fixed);
}

export function buildSubstituteOperandResolver(
  ctx: PreprocessContext,
  resolvePositionSize: (tok: string) => number,
  resolvePriceAnchor: (tok: string) => number,
  resolveAccountNumber: (tok: string) => number,
  resolveSynthesized: (tok: string) => number,
  positionSizeConstants: Set<string>,
  priceAnchorConstants: Set<string>,
  accountNumberConstants: Set<string>,
  synthesizedConstants: Set<string>,
  // Live-var recursion stack, threaded by substituteTokens so a math
  // operand `@<liveVar>` participates in cycle detection just like a
  // bare `@<liveVar>` token would.
  seen: Set<string> = new Set(),
  // Names that an earlier step in this chain will `captureTo`.
  // Capture targets are comma-lists of order IDs, not numbers —
  // doing math on them doesn't make sense. Reject with a clear
  // message rather than throw "Unknown variable" later.
  expectedFutureCaptures?: Set<string>,
): MathOperandResolver {
  return (operand: string): number => {
    // Numeric literal (with optional unary minus).
    if (/^-?(?:\d+(?:\.\d+)?|\.\d+)$/.test(operand)) {
      return parseFloat(operand);
    }
    if (operand.startsWith('$')) {
      if (positionSizeConstants.has(operand)) return resolvePositionSize(operand);
      if (priceAnchorConstants.has(operand)) return resolvePriceAnchor(operand);
      if (accountNumberConstants.has(operand)) return resolveAccountNumber(operand);
      if (synthesizedConstants.has(operand)) return resolveSynthesized(operand);
      throw new Error(`math: unknown constant ${operand}`);
    }
    if (operand.startsWith('@')) {
      const name = operand.slice(1);
      if (expectedFutureCaptures?.has(name)) {
        throw new Error(
          `math: @${name} is a pending capture target — capture vars hold a comma-list of order IDs, not a number. Use @${name} bare, not in a math expression.`,
        );
      }
      // Tracked var binding first — matches substituteTokens'
      // precedence. The resolver re-evaluates the source and may
      // update the extreme as a side-effect.
      const trackedEntry = ctx.store.getTrackedVar(name);
      if (trackedEntry) {
        return resolveTrackedVar(name, trackedEntry, ctx, seen);
      }
      // Live binding wins over static — same precedence as
      // substituteTokens. Mutual exclusion in the storage layer
      // guarantees we never see both.
      const liveSource = ctx.store.getLiveVar(name);
      if (liveSource !== undefined) {
        return evaluateLiveVarSource(name, liveSource, ctx, seen);
      }
      const raw = ctx.store.getVar(name);
      if (raw === undefined) {
        throw new Error(`math: unknown variable @${name}`);
      }
      const n = parseFloat(raw);
      if (!Number.isFinite(n)) {
        throw new Error(`math: variable @${name}='${raw}' is not numeric`);
      }
      return n;
    }
    throw new Error(`math: unrecognized operand '${operand}'`);
  };
}
