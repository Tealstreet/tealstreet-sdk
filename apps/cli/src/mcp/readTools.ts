import type { ReadMethodName } from '@tealstreet/script-api-core';
import type { ZodRawShape } from 'zod';

import { boundReadResult, NARROWING_BY_METHOD, SHAPE_BY_METHOD } from '@tealstreet/oems-core';
import { CORE_READ_BINDINGS } from '@tealstreet/script-api-core/src/runtime/readBindings';
import { z } from 'zod';

/**
 * The CLI's projection of the SDK read surface.
 *
 * The definitions come from `readBindings.ts` — the same source the web agent
 * projects, though it takes the generated manifest rather than the bindings,
 * because it only describes the surface and this server also calls it. Either
 * way the two cannot describe one read two ways. What lives here is only what
 * is CLI-specific: which reads to expose, and the tool names.
 *
 * ## The names stay snake_case
 *
 * `get_positions`, not `getPositions`. They are in users' saved prompts and in
 * `apps/documentation/cli/reference/commands.md`. The BINDINGS supply the
 * definition; a shell owns its own naming, which is the whole point of
 * projecting rather than re-exporting.
 *
 * ## Why not all fifteen CLI reads
 *
 * `getActiveAccount`, `getActiveSymbol` and `getActiveExchange` resolve from a
 * `CLISession`. The MCP server is a SECOND process with no REPL session of its
 * own, so it would have to invent one per call — and would then answer
 * "the active symbol is ''" with total confidence. A tool that always lies is
 * worse than a missing tool.
 *
 * `getActiveAccounts` and `getCombinedBalance` take no `account`, so
 * `ensureConnected` cannot run for them: they would silently report on
 * whichever accounts happened to be connected already, which reads as an
 * answer and is not one.
 *
 * `getCandles` is left out because `get_exchange_proxy_candles` already serves
 * candles here, and two candle tools whose difference is "live vs proxy" is a
 * choice no client will make correctly.
 *
 * `list_accounts` stays hand-written in `server.ts`: it reads account CONFIG,
 * including `testnet`, which the SDK's `getAccounts` does not carry. It was
 * never part of the store bypass this file exists to remove.
 */
export const CLI_READ_TOOLS: Readonly<Record<string, ReadMethodName>> = Object.freeze({
  get_positions: 'getPositions',
  get_orders: 'getOrders',
  get_balance: 'getBalance',
  get_ticker: 'getTicker',
  get_tickers: 'getTickers',
  get_markets: 'getMarkets',
  get_orderbook: 'getOrderbook',
});

/**
 * The manifest's JSON Schema, as the zod shape the MCP SDK wants.
 *
 * `z.fromJSONSchema` reads our dialect verbatim — `type: ['string','null']`
 * and `enum` included — so this is a conversion, not a schema DSL of its own.
 * That distinction matters: a hand-built universal schema layer is the thing
 * `docs/plans/SDK_SURFACE_VISION_v1.md` warns against, and a stdlib call is not
 * one.
 *
 * The one adjustment: a nullable field arrives REQUIRED, because that is what
 * OpenAI strict mode needs and what the manifest therefore encodes. An MCP
 * client should be able to omit an optional argument rather than send an
 * explicit `null`, so nullable fields are relaxed to `.nullish()`.
 */
export const readToolShape = (method: ReadMethodName): ZodRawShape => {
  const params = CORE_READ_BINDINGS[method].params;
  // Cast through `unknown`: zod 4 types `fromJSONSchema`'s result as the core
  // `$ZodType`, which has no `.nullish()`. The runtime value is a real
  // `ZodObject` whose members are `ZodTypeAny`.
  const converted = z.fromJSONSchema(params as never) as unknown as { shape: Record<string, z.ZodTypeAny> };
  const shape: Record<string, z.ZodTypeAny> = {};
  for (const [name, field] of Object.entries(params.properties)) {
    const types = Array.isArray(field.type) ? field.type : [field.type];
    // `.optional()`, not `.nullish()`. The converted field is ALREADY a
    // string-or-null union, so `.nullish()` wraps it a second time and the MCP
    // SDK emits a nested `anyOf` with the description buried inside the inner
    // arm — where most clients will not show it. `.optional()` produces one
    // `anyOf`, keeps the description on the property, and drops out of
    // `required` just the same.
    shape[name] = types.includes('null') ? converted.shape[name].optional() : converted.shape[name];
  }
  return shape;
};

/**
 * Rendering, which is the shell's own job — the manifest supplies the
 * definition, not the presentation.
 *
 * Three things the hand-written tools did that a raw JSON dump would lose, and
 * each was found by comparing old output to new rather than by reading the
 * code:
 *
 * 1. **Zero-size positions are filtered.** `CoreAdapter.getPositions` does NOT
 *    filter them — it returns the store's list as-is — and a flat position is
 *    not a position. `buildOemsContext` on the web filters the same way
 *    (`isLive`), so this is the shared expectation rather than a CLI quirk.
 * 2. **`info` is stripped and the result is bounded.** `Position`, `Ticker`,
 *    `Market` and `Order` all carry `info?: any`, the raw venue payload. The
 *    old prose picked named fields and never saw it; a JSON dump would ship
 *    megabytes down a stdio transport. `boundReadResult` is the same control
 *    the web agent uses, for the same reason.
 * 3. **Empty is distinguished from still-loading.** Stores hydrate
 *    asynchronously after connect, so `[]` right after a cold connect is
 *    ambiguous and "none" would be a wrong answer.
 */
/**
 * What a client can do about a truncated result, off the read's own params.
 *
 * `get_orderbook` is the trap: it takes `depth` and looks narrowable, but the
 * 20-level cap is applied after the venue answers, so a larger `depth` returns
 * the same twenty.
 */
const adviceFor = (method: ReadMethodName): string => {
  if (SHAPE_BY_METHOD.get(method) === 'orderbook') return 'These are the levels nearest the touch on each side.';
  switch (NARROWING_BY_METHOD.get(method)) {
    case 'window':
      return 'Ask again with from, to or limit for a different window.';
    case 'symbol':
      return 'Ask again naming a symbol to see one that is not here.';
    default:
      return 'There is no way to page past this.';
  }
};

export const renderReadResult = (method: ReadMethodName, data: unknown): string => {
  const filtered =
    method === 'getPositions' && Array.isArray(data)
      ? data.filter((position) => (position as { contracts?: number }).contracts !== 0)
      : data;

  if (filtered === undefined || filtered === null || (Array.isArray(filtered) && filtered.length === 0)) {
    return `No result from ${method}. If this account just connected, its data may still be loading — try again in a moment.`;
  }

  const { data: bounded, truncated, kept, total } = boundReadResult(method, filtered);
  // The COUNTS, not just the fact. `capArray` keeps a tail, which is right for
  // candles and arbitrary for a 600-market venue — so a client that is told
  // "60 of 612" can narrow its request, where one told only "truncated" will
  // reason over the tail as if it were the set.
  //
  // They come FROM the bounding rather than being recomputed here. This file
  // used to derive them from its own copy of the input, which is how the same
  // truncation came to be described two ways by the two shells that render it.
  //
  // The advice comes from the read's own params for the same reason it does in
  // the web agent's prompt: "narrow the request" was appended to EVERY read
  // here, including `get_markets`, which takes an account and nothing else.
  const note = truncated
    ? `\n\n(truncated — showing ${kept ?? 'part'}${total === undefined ? '' : ` of ${total}`}. ${adviceFor(method)})`
    : '';
  return `${JSON.stringify(bounded, null, 2)}${note}`;
};
