import type { Timeframe } from '@tealstreet/safe-cex/src/types';
import type { CoreAdapter, MaybePromise } from '../adapter/AdapterContext';
import type { ReadMethodName } from '../api/capabilities.types';
import type { ApiPath } from './runtimeSurface';

/**
 * The read surface, described in a form a machine can consume.
 *
 * `CoreAdapter` says what a read IS. This says what a read LOOKS LIKE to a
 * caller that has to name it in JSON — a tool schema, a lint manifest, an LLM
 * function list. `generate-manifest.ts` strips `invoke` and emits the rest;
 * shells that execute import the binding and call it.
 *
 * ## Why this is not derived from the TypeScript signatures
 *
 * It cannot be, faithfully. `CoreAdapter` reads take positional arguments with
 * an already-resolved account (`getCandles(symbol, timeframe, opts)`), while a
 * tool takes ONE flat object. The flattening is a real mapping with real
 * choices in it, so it is written down once, here, rather than guessed
 * separately by every shell — which is exactly what the CLI stdio MCP, the
 * hosted MCP and the web agent each did before this file existed.
 *
 * ## Params are OpenAI strict-mode JSON Schema, by construction
 *
 * `objectSchema` puts every property in `required` and sets
 * `additionalProperties: false`, because strict mode has no optional property
 * — an optional field is a required one whose type includes `"null"`. The same
 * rule `packages/oems-core/src/planSchema.ts` documents at length. Building it
 * into the helper means a binding cannot express a schema the provider will
 * reject at request time.
 *
 * ## No zod, deliberately
 *
 * `script-api-core` depends on `@tealstreet/math`, `@tealstreet/safe-cex` and
 * `@tealstreet/utils` and nothing else, and is consumed as SOURCE by both V2
 * and V3 under the tealchart/tealscript compatibility carveout. Raising a
 * schema DSL into that dependency set to save some object literals would be a
 * bad trade — and JSON Schema is what every consumer wants anyway.
 */

// ── JSON Schema, the small subset a read parameter needs ──────────────────

export interface JsonSchemaField {
  readonly type: string | readonly string[];
  readonly description: string;
  readonly enum?: readonly (string | null)[];
  readonly items?: { readonly type: string };
}

export interface JsonSchemaObject {
  readonly type: 'object';
  readonly properties: Readonly<Record<string, JsonSchemaField>>;
  readonly required: readonly string[];
  readonly additionalProperties: false;
}

/** Every key lands in `required`; optionality is expressed by a nullable type. */
const objectSchema = (properties: Readonly<Record<string, JsonSchemaField>>): JsonSchemaObject => ({
  type: 'object',
  properties,
  required: Object.keys(properties),
  additionalProperties: false,
});

const NO_PARAMS = objectSchema({});

const text = (description: string): JsonSchemaField => ({ type: 'string', description });
const nullableText = (description: string): JsonSchemaField => ({ type: ['string', 'null'], description });
const nullableCount = (description: string): JsonSchemaField => ({ type: ['number', 'null'], description });
const textList = (description: string): JsonSchemaField => ({
  type: 'array',
  items: { type: 'string' },
  description,
});
const nullableTextList = (description: string): JsonSchemaField => ({
  type: ['array', 'null'],
  items: { type: 'string' },
  description,
});

const ACCOUNT = text('Account name, exactly as it appears in the account list.');

/**
 * `Timeframe` from safe-cex, as a runtime list.
 *
 * Duplicated from the type because a type cannot be enumerated at runtime;
 * `assertTimeframe` below is what keeps them honest, and the compile-time
 * `satisfies readonly Timeframe[]` fails here if safe-cex drops a member.
 */
const TIMEFRAMES = [
  '1m',
  '3m',
  '5m',
  '15m',
  '30m',
  '1h',
  '2h',
  '4h',
  '6h',
  '12h',
  '1d',
  '1w',
] as const satisfies readonly Timeframe[];

// ── Reading arguments that arrived as untyped JSON ────────────────────────

type Args = Readonly<Record<string, unknown>>;

const requireText = (args: Args, key: string): string => {
  const value = args[key];
  if (typeof value !== 'string' || value.trim() === '') throw new Error(`"${key}" is required and must be a string`);
  return value;
};

const optionalText = (args: Args, key: string): string | undefined => {
  const value = args[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'string') throw new Error(`"${key}" must be a string`);
  return value.trim() === '' ? undefined : value;
};

const optionalCount = (args: Args, key: string): number | undefined => {
  const value = args[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'number' || !Number.isFinite(value)) throw new Error(`"${key}" must be a number`);
  return value;
};

const requireTextList = (args: Args, key: string): readonly string[] => {
  const value = args[key];
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== 'string')) {
    throw new Error(`"${key}" is required and must be an array of strings`);
  }
  return value as readonly string[];
};

const optionalTextList = (args: Args, key: string): readonly string[] | undefined => {
  const value = args[key];
  if (value === undefined || value === null) return undefined;
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== 'string')) {
    throw new Error(`"${key}" must be an array of strings`);
  }
  return value as readonly string[];
};

const requireTimeframe = (args: Args, key: string): Timeframe => {
  const value = requireText(args, key);
  if (!(TIMEFRAMES as readonly string[]).includes(value)) {
    throw new Error(`"${key}" must be one of: ${TIMEFRAMES.join(', ')}`);
  }
  return value as Timeframe;
};

// ── The bindings ──────────────────────────────────────────────────────────

/**
 * `store` answers from state the host already holds; `network` may reach the
 * venue. Shells use it to decide what to rate-limit and what an agent may
 * chain freely — not a cache hint.
 */
export type ReadCostClass = 'store' | 'network';

/**
 * The SHAPE of what a read returns, which is what decides how to bound it.
 *
 * A shell that hands a result to a model has to cap it, and the cap differs by
 * shape: a candle series wants its most recent window, a book wants the levels
 * nearest the touch on each side, a row list wants a plain count, and a single
 * object has nothing to slice. That is a property of the READ, so it belongs
 * on the binding — beside `costClass`, which answers the same kind of question
 * about the same subject.
 *
 * It lives here because the alternative was measured and was worse:
 * `boundReadResult` decided by string-matching `'getOrderbook'` and
 * `'getCandles'` against a parameter typed `string`. Renaming a method updated
 * the manifest, the agent's tool list, the prompt block, the CLI registry and
 * every drift test — and silently dropped the book cap, with no compile error
 * and no failing test, leaving a 5,000-level book to ride into a prompt under
 * the character backstop alone.
 */
export type ReadResultShape = 'candles' | 'orderbook' | 'rows' | 'scalar';

export interface ReadBinding {
  readonly method: ReadMethodName;
  readonly path: ApiPath;
  readonly summary: string;
  readonly params: JsonSchemaObject;
  readonly costClass: ReadCostClass;
  readonly resultShape: ReadResultShape;
  /**
   * Bound to `CoreAdapter`, not `CoreApi`, because the callers are HOSTS —
   * the CLI's MCP server, the browser's agent executor — and both already hold
   * one. `CoreApi`'s optional-account sugar exists for user-authored scripts,
   * where an omitted account should resolve to the active one; a tool argument
   * that quietly means "whichever account was selected" is a footgun in an
   * agent transcript, so `account` is required wherever it applies.
   */
  readonly invoke: (adapter: CoreAdapter, args: Args) => MaybePromise<unknown>;
}

export const CORE_READ_BINDINGS: Readonly<Record<ReadMethodName, ReadBinding>> = Object.freeze({
  getActiveAccount: {
    method: 'getActiveAccount',
    path: 'api.getActiveAccount',
    summary: 'The account currently selected in the interface.',
    params: NO_PARAMS,
    costClass: 'store',
    resultShape: 'scalar',
    invoke: (adapter) => adapter.getActiveAccount(),
  },
  getActiveAccounts: {
    method: 'getActiveAccounts',
    path: 'api.getActiveAccounts',
    summary: 'Every account currently streaming live data.',
    params: NO_PARAMS,
    costClass: 'store',
    resultShape: 'rows',
    invoke: (adapter) => adapter.getActiveAccounts(),
  },
  getActiveSymbol: {
    method: 'getActiveSymbol',
    path: 'api.getActiveSymbol',
    summary: 'The symbol currently selected in the interface.',
    params: NO_PARAMS,
    costClass: 'store',
    resultShape: 'scalar',
    invoke: (adapter) => adapter.getActiveSymbol(),
  },
  getActiveExchange: {
    method: 'getActiveExchange',
    path: 'api.getActiveExchange',
    summary: 'The exchange of the currently selected account.',
    params: NO_PARAMS,
    costClass: 'store',
    resultShape: 'scalar',
    invoke: (adapter) => adapter.getActiveExchange(),
  },
  getAccounts: {
    method: 'getAccounts',
    path: 'api.getAccounts',
    summary: 'Configured accounts: name, exchange, and whether each is streaming. Never credentials.',
    params: NO_PARAMS,
    costClass: 'store',
    resultShape: 'rows',
    invoke: (adapter) => adapter.getAccounts(),
  },
  getMarkets: {
    method: 'getMarkets',
    path: 'api.getMarkets',
    summary: 'Every tradeable market on one account, with its precision and limits.',
    params: objectSchema({ account: ACCOUNT }),
    costClass: 'store',
    resultShape: 'rows',
    invoke: (adapter, args) => adapter.getMarkets(requireText(args, 'account')),
  },
  getMarket: {
    method: 'getMarket',
    path: 'api.getMarket',
    summary: 'One market: tick size, lot step, contract size and limits.',
    params: objectSchema({ symbol: text('Market symbol, e.g. BTCUSDT.'), account: ACCOUNT }),
    costClass: 'store',
    resultShape: 'scalar',
    invoke: (adapter, args) => adapter.getMarket(requireText(args, 'symbol'), requireText(args, 'account')),
  },
  getTicker: {
    method: 'getTicker',
    path: 'api.getTicker',
    summary: 'Current ticker for one symbol: last, bid, ask, mark, 24h change and volume.',
    params: objectSchema({ symbol: text('Market symbol, e.g. BTCUSDT.'), account: ACCOUNT }),
    costClass: 'store',
    resultShape: 'scalar',
    invoke: (adapter, args) => adapter.getTicker(requireText(args, 'symbol'), requireText(args, 'account')),
  },
  getTickers: {
    method: 'getTickers',
    path: 'api.getTickers',
    summary: 'Current tickers for several symbols at once.',
    params: objectSchema({ symbols: textList('Market symbols.'), account: ACCOUNT }),
    costClass: 'store',
    resultShape: 'rows',
    invoke: (adapter, args) => adapter.getTickers(requireTextList(args, 'symbols'), requireText(args, 'account')),
  },
  getPositions: {
    method: 'getPositions',
    path: 'api.getPositions',
    summary: 'Open positions on one account: side, size, entry price and unrealized PnL.',
    params: objectSchema({
      account: ACCOUNT,
      symbol: nullableText('Restrict to one symbol. Null means every symbol held.'),
    }),
    costClass: 'store',
    resultShape: 'rows',
    invoke: (adapter, args) => {
      const symbol = optionalText(args, 'symbol');
      return adapter.getPositions({ account: requireText(args, 'account'), ...(symbol ? { symbol } : {}) });
    },
  },
  getHistoricalPosition: {
    method: 'getHistoricalPosition',
    path: 'api.getHistoricalPosition',
    summary: 'The most recent CLOSED position for one symbol on one account.',
    params: objectSchema({ account: ACCOUNT, symbol: text('Market symbol.') }),
    costClass: 'network',
    resultShape: 'scalar',
    invoke: (adapter, args) =>
      adapter.getHistoricalPosition({ account: requireText(args, 'account'), symbol: requireText(args, 'symbol') }),
  },
  getOrders: {
    method: 'getOrders',
    path: 'api.getOrders',
    summary: 'Resting orders on one account: id, side, type, size, price and reduce-only flag.',
    params: objectSchema({
      account: ACCOUNT,
      symbol: nullableText('Restrict to one symbol. Null means every symbol.'),
    }),
    costClass: 'store',
    resultShape: 'rows',
    invoke: (adapter, args) => {
      const symbol = optionalText(args, 'symbol');
      return adapter.getOrders({ account: requireText(args, 'account'), ...(symbol ? { symbol } : {}) });
    },
  },
  getBalance: {
    method: 'getBalance',
    path: 'api.getBalance',
    summary: 'Balance for one account: total, free, used and unrealized PnL.',
    params: objectSchema({ account: ACCOUNT }),
    costClass: 'store',
    resultShape: 'scalar',
    invoke: (adapter, args) => adapter.getBalance(requireText(args, 'account')),
  },
  getCombinedBalance: {
    method: 'getCombinedBalance',
    path: 'api.getCombinedBalance',
    summary: 'Balances summed across accounts.',
    params: objectSchema({
      accounts: nullableTextList('Account names to sum. Null means every streaming account.'),
    }),
    costClass: 'store',
    resultShape: 'scalar',
    invoke: (adapter, args) => {
      const accounts = optionalTextList(args, 'accounts');
      return adapter.getCombinedBalance(accounts ? { accounts } : {});
    },
  },
  getOrderbook: {
    method: 'getOrderbook',
    path: 'api.getOrderbook',
    summary: 'Current bid and ask ladders for one symbol.',
    params: objectSchema({
      symbol: text('Market symbol.'),
      account: ACCOUNT,
      depth: nullableCount('Levels per side. Null means the host default.'),
    }),
    costClass: 'store',
    resultShape: 'orderbook',
    invoke: (adapter, args) => {
      const depth = optionalCount(args, 'depth');
      return adapter.getOrderbook(requireText(args, 'symbol'), {
        account: requireText(args, 'account'),
        ...(depth === undefined ? {} : { depth }),
      });
    },
  },
  getCandles: {
    method: 'getCandles',
    path: 'api.getCandles',
    summary: 'Historical OHLCV candles for one symbol and timeframe.',
    params: objectSchema({
      symbol: text('Market symbol.'),
      timeframe: { type: 'string', enum: TIMEFRAMES, description: 'Candle interval.' },
      account: ACCOUNT,
      from: nullableCount('Start time, epoch milliseconds. Null means the host default window.'),
      to: nullableCount('End time, epoch milliseconds. Null means now.'),
      limit: nullableCount('Maximum candles to return. Null means the host default.'),
    }),
    costClass: 'network',
    resultShape: 'candles',
    invoke: (adapter, args) => {
      const from = optionalCount(args, 'from');
      const to = optionalCount(args, 'to');
      const limit = optionalCount(args, 'limit');
      return adapter.getCandles(requireText(args, 'symbol'), requireTimeframe(args, 'timeframe'), {
        account: requireText(args, 'account'),
        ...(from === undefined ? {} : { from }),
        ...(to === undefined ? {} : { to }),
        ...(limit === undefined ? {} : { limit }),
      });
    },
  },
});

/**
 * A binding with `invoke` removed — the serialisable half.
 *
 * This is what `generate-manifest.ts` emits and what a shell that only needs
 * to DESCRIBE the surface consumes. A shell that EXECUTES imports the binding
 * itself; the manifest deliberately cannot call anything.
 */
export type ReadManifestEntry = Omit<ReadBinding, 'invoke'>;

export const toManifestEntry = ({
  method,
  path,
  summary,
  params,
  costClass,
  resultShape,
}: ReadBinding): ReadManifestEntry => ({
  method,
  path,
  summary,
  params,
  costClass,
  resultShape,
});
