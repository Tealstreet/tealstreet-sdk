import type { ReadResultShape } from '@tealstreet/script-api-core/src/runtime/readBindings';

import { CORE_READ_MANIFEST } from '@tealstreet/script-api-core/src/runtime/read-manifest';

/**
 * How a read's result is shaped, off the MANIFEST rather than off a method
 * name written out here.
 *
 * `boundReadResult` used to decide by string-matching `'getOrderbook'` and
 * `'getCandles'` against a parameter typed `string` — the last hand-written
 * copy of the SDK read vocabulary, sitting inside the package that owns the
 * bounding. Renaming a method updated the manifest, the agent tool list, the
 * prompt block, the CLI registry and every drift test, and silently dropped
 * the cap here: a book fell to the array branch (it is an object, so that is
 * false), skipped the halving loop, and only the character backstop remained.
 *
 * ## Why its own file
 *
 * Both `boundContext.ts` (which caps by shape) and `prompt.ts` (which words
 * the truncation marker by shape) need it, and `boundContext.ts` already
 * imports `OemsReadResult` from `prompt.ts`. Putting the map in either of them
 * makes that pair a cycle.
 */
export const SHAPE_BY_METHOD: ReadonlyMap<string, ReadResultShape> = new Map(
  CORE_READ_MANIFEST.map((entry) => [entry.method as string, entry.resultShape]),
);

/**
 * `Object.hasOwn`, not `in`. `in` walks the prototype chain, so a polluted
 * `Object.prototype.bids` would classify every object result as a book — and
 * on the unknown-tool path that replaces the real payload with `{bids, asks}`.
 * A known shape already guards the branch; this closes the fallback.
 */
export const isBook = (value: unknown): value is { bids?: unknown[]; asks?: unknown[] } =>
  value !== null &&
  typeof value === 'object' &&
  !Array.isArray(value) &&
  (Object.hasOwn(value, 'bids') || Object.hasOwn(value, 'asks'));

/**
 * An unknown `tool` is NOT treated as an error.
 *
 * `boundReadResults` runs in the route before anything validates the name, so
 * a forged body and a client one deploy behind both land here. Throwing would
 * not even surface honestly: `readBody` sits inside the route's `JSON.parse`
 * try, so any raise from in here is answered as "Request body must be JSON" —
 * a 400 about the wrong thing. The value's own shape decides instead, which is
 * what the string-matching version did by accident and is the conservative
 * answer.
 */
export const shapeFor = (tool: string, value: unknown): ReadResultShape =>
  SHAPE_BY_METHOD.get(tool) ?? (isBook(value) ? 'orderbook' : Array.isArray(value) ? 'rows' : 'scalar');

/**
 * What a caller can do to see the part that was dropped.
 *
 * Derived from the read's own PARAMS rather than from its shape, because the
 * two do not line up: `getMarkets`, `getPositions` and `getTickers` are all
 * `rows`, but only the last two take a symbol argument, and a model told
 * "there is no way to page past this" would not go on to ask about the symbols
 * it can already see in the snapshot.
 *
 * `getOrderbook` is called out separately because it LOOKS narrowable — it has
 * `depth` — and is not: the 20-level cap is applied after the venue answers,
 * so a larger `depth` returns the same twenty.
 */
export type ReadNarrowing = 'window' | 'symbol' | 'none';

const narrowingOf = (params: readonly string[], shape: ReadResultShape): ReadNarrowing => {
  if (shape === 'orderbook') return 'none';
  if (params.some((name) => name === 'from' || name === 'to' || name === 'limit')) return 'window';
  if (params.some((name) => name === 'symbol' || name === 'symbols')) return 'symbol';
  return 'none';
};

export const NARROWING_BY_METHOD: ReadonlyMap<string, ReadNarrowing> = new Map(
  CORE_READ_MANIFEST.map((entry) => [
    entry.method as string,
    narrowingOf(Object.keys(entry.params.properties), entry.resultShape),
  ]),
);
