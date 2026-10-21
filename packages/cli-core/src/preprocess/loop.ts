import { PositionSide } from '@tealstreet/safe-cex/src/types';
import type { PreprocessContext } from './types';

/**
 * `loop` over positions — preprocess-time fan-out across symbols.
 *
 * Shape:   `loop <variant>: <body>`
 * Where:   variant ∈ { longs, shorts, openpos, open, allpos, all }
 *
 * The body extends to the end of the segment (`splitChain` already
 * split the chain). For every position whose side matches the variant
 * (and `contracts !== 0`), we emit one expanded step: `<body> <symbol>`.
 * The trailing symbol is what most commands use to scope, so this
 * works for `loop longs: buy 1$`, `loop shorts: close`, etc.
 *
 * The match is intentionally narrow — multi-statement bodies are NOT
 * supported (they'd require paren-tracking that doesn't pay off
 * compared to "alias the body, then loop the alias name"). If the
 * body itself contains a `;` it's because the surrounding alias /
 * caller already split, and only the first statement reaches us.
 */

const LOOP_PREFIX_RE = /^loop\s+([a-z]+)\s*:\s*(.*)$/i;

export interface LoopExpansion {
  /** Pre-substitution body strings, one per matching position. Empty
   *  array signals "no positions matched the variant — skip silently". */
  expansions: string[];
}

/**
 * Try to interpret `raw` as a `loop <variant>: <body>` clause. Returns
 * `null` if the segment isn't a loop, otherwise an array of body
 * strings (already with the symbol appended) ready to run through the
 * normal substitution pipeline.
 */
export function tryExpandLoop(
  raw: string,
  ctx: PreprocessContext,
): LoopExpansion | null {
  const m = raw.trim().match(LOOP_PREFIX_RE);
  if (!m) return null;

  const variant = m[1].toLowerCase();
  const body = m[2].trim();
  if (!body) {
    throw new Error(`loop: empty body after '${variant}:'`);
  }

  const symbols = pickLoopSymbols(variant, ctx);
  if (symbols === null) {
    throw new Error(
      `loop: unknown variant '${variant}'. Supported: longs, shorts, openpos / open, allpos / all`,
    );
  }
  return {
    expansions: symbols.map((sym) => `${body} ${sym}`),
  };
}

function pickLoopSymbols(
  variant: string,
  ctx: PreprocessContext,
): string[] | null {
  ctx.exchange?.syncAccountState?.();
  const positions = (ctx.exchange?.store?.positions ?? []).filter(
    (p) => p.contracts !== 0,
  );
  switch (variant) {
    case 'longs':
      return uniqueSymbols(positions.filter((p) => p.side === PositionSide.Long));
    case 'shorts':
      return uniqueSymbols(positions.filter((p) => p.side === PositionSide.Short));
    case 'openpos':
    case 'open':
    case 'allpos':
    case 'all':
      return uniqueSymbols(positions);
    default:
      return null;
  }
}

function uniqueSymbols(positions: { symbol: string }[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const p of positions) {
    if (seen.has(p.symbol)) continue;
    seen.add(p.symbol);
    out.push(p.symbol);
  }
  return out;
}
