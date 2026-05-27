import { parseConditionText } from './conditions';
import { peelParenBlock } from './parserUtils';
import type { PreprocessContext } from './types';

/**
 * `if (<cond>) (<then>) [else (<else>)]` — preprocess-time
 * conditional. Evaluates the condition against current
 * `ctx.exchange.store` once, picks the matching branch, and emits
 * the branch body verbatim. The body is a normal chain expression
 * (`splitChain` handles `;` / `&` / nested parens), so a branch can
 * contain multiple statements.
 *
 * Intentionally NOT reactive — the condition is checked exactly
 * once. Use `wait <cond>; …` for "block until true, then run." Use
 * `repeat N (if (<cond>) (do thing))` to re-check N times.
 *
 * Returns `null` when the segment isn't an if-form. Throws on a
 * malformed if-form so the user sees a clear error rather than
 * having the body silently treated as a literal command.
 */
export interface IfExpansion {
  /** The chain text to splice into the chain in place of the if-segment.
   *  Empty when the condition was false and no else branch was given. */
  body: string;
}

const IF_HEAD_RE = /^if\s*\(/i;
const ELSE_KEYWORD_RE = /^else\b/i;

export function tryExpandIfElse(
  segment: string,
  ctx: PreprocessContext,
): IfExpansion | null {
  const trimmed = segment.trim();
  if (!IF_HEAD_RE.test(trimmed)) return null;

  // Walk: `if (<cond>) (<then>) [else (<else>)]`.
  const afterIf = trimmed.slice(2).trimStart(); // past 'if'

  const cond = peelParenBlock(afterIf);
  if (!cond) {
    throw new Error(`if: malformed — expected '(<condition>)' after 'if'`);
  }
  const afterCond = afterIf.slice(cond.consumed).trimStart();

  const thenBlock = peelParenBlock(afterCond);
  if (!thenBlock) {
    throw new Error(`if: malformed — expected '(<then>)' after the condition`);
  }
  const afterThen = afterCond.slice(thenBlock.consumed).trimStart();

  let elseBlock: { body: string; consumed: number } | null = null;
  if (afterThen.length > 0) {
    if (!ELSE_KEYWORD_RE.test(afterThen)) {
      throw new Error(
        `if: unexpected trailing tokens after then-branch — got '${afterThen}'`,
      );
    }
    const afterElseKeyword = afterThen.slice(4).trimStart();
    elseBlock = peelParenBlock(afterElseKeyword);
    if (!elseBlock) {
      throw new Error(`if: 'else' must be followed by '(<else-body>)'`);
    }
    const tail = afterElseKeyword.slice(elseBlock.consumed).trim();
    if (tail.length > 0) {
      throw new Error(`if: unexpected trailing tokens after else-branch — got '${tail}'`);
    }
  }

  // Parse + evaluate the condition. Goes through parseConditionText
  // so the condition body can use `either`/`both` combinators.
  const parsed = parseConditionText(cond.body);
  if (!parsed) {
    throw new Error(`if: unrecognized condition '${cond.body}'`);
  }
  if (!ctx.exchange) {
    throw new Error(`if: not connected to an exchange (condition needs market state)`);
  }
  // Pass the store so `if (@<liveVar> > N) (…)` resolves at evaluation
  // time. Existing condition kinds ignore the context.
  const result = parsed.check(ctx.exchange, ctx.symbol, { store: ctx.store });

  return { body: result ? thenBlock.body : (elseBlock?.body ?? '') };
}
