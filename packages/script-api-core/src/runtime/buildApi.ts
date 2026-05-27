/**
 * `buildApi` — assembles the CoreApi object the script IIFE receives as
 * its second argument. See SPEC §2.1.
 *
 * Phase 2 wires:
 *   - read-side getters → `proxy.call`  (with `validateReadCapability`)
 *   - write-side actions → `proxy.callWrite`  (with `validateWriteCapability`
 *     + `validateExchangeCapability` + account resolution)
 *   - `runCommand` → write seam with extra verb-allowlist + depth tracking
 *     (`validateRunCommandCapability`)
 *   - `api.io.{print,return}` → in-memory sink the dispatcher reads back
 *
 * `api.storage.*` stays stubbed until Phase 3.
 *
 * The returned `api` is *not* deep-frozen — `api.io` carries a
 * stateful sink (the printer / return slot) the dispatcher reads back
 * after the script promise resolves. That mutability is contained to
 * the io sub-object; everything else is freeze-safe.
 */
import { CORE_CONSTANTS } from '../constants';
import { CORE_UTILS } from '../utils';
import { CORE_API_VERSION } from '../version';
import type {
  AccountOpts,
  CombinedBalanceOpts,
  CoreApi,
  CoreExchange,
  CoreIo,
  CoreMeta,
  CoreStorage,
  HistoricalPositionQuery,
  OrderQueryOpts,
  OrderbookOpts,
  PlaceOrderOptions,
  PositionOpts,
  PrintKind,
  Timeframe,
} from '../api/CoreApi.types';
import type { Capabilities } from '../api/capabilities.types';
import type { InvokedFrom } from '../api/CoreApi.types';
import type { ApiProxy } from '../proxy/ApiProxy';
import { resolveAccount } from './accountResolver';
import {
  validateExchangeCapability,
  validateReadCapability,
  validateRunCommandCapability,
  validateWriteCapability,
} from './capabilityCheck';

/**
 * Mutable IO sink. The dispatcher constructs one per script invocation,
 * threads it into `buildApi`, and reads it back after the script
 * resolves.
 */
export interface ScriptIoSink {
  /** Every line the script emitted via `api.io.print`. */
  readonly output: { text: string; kind: PrintKind }[];
  /**
   * The latest `api.io.return(value)` call. `null` is a sentinel for
   * "no return yet". `api.io.return` can be called multiple times; the
   * dispatcher uses the final value at resolve time per SPEC §7.4.
   */
  hasReturn: boolean;
  returnValue: unknown;
}

export const createScriptIoSink = (): ScriptIoSink => ({
  output: [],
  hasReturn: false,
  returnValue: undefined,
});

/**
 * Inputs needed to assemble `api`. The dispatcher (`runScript`)
 * computes these once per invocation and hands them off.
 */
export interface BuildApiInputs {
  readonly proxy: ApiProxy;
  readonly capabilities: Capabilities;
  /** Host's current active account at invocation time. Used as the
   *  last-resort fallback by `resolveAccount`. May be empty string when
   *  the host has no active account; resolution then throws. */
  readonly activeAccount: string;
  /**
   * `account → exchange` lookup the dispatcher pre-resolved from the
   * adapter's `getAccounts()`. Lets `validateExchangeCapability` fire
   * synchronously at every write site without re-querying. Hosts that
   * haven't yet hydrated their accounts can pass an empty map; the
   * exchange capability check then short-circuits as a no-op (only
   * tightening would be wrong-deny, never wrong-allow).
   */
  readonly accountToExchange: Readonly<Record<string, string>>;
  readonly sink: ScriptIoSink;
  readonly meta: CoreMeta;
  /** The host's print sink. Standalone wires `adapter.print`; tests can
   *  pass a stub. The IO slice fires this side-by-side with the in-memory
   *  sink so the dispatcher gets a clean transcript even if the host's
   *  print errors. */
  readonly printingHost: PrintingHost;
  /**
   * Nested runCommand depth inherited from a parent dispatch. Omitted for
   * top-level script invocation, where the next runCommand call is depth 1.
   */
  readonly runCommandDepth?: number;
}

/**
 * Build the `api.io` slice. `print` does two things in tandem:
 *   1. records the line on the in-memory sink so the dispatcher can read
 *      the full transcript back for log mirroring (`[hook:<name>] …`
 *      prefix on webhook fires per SPEC §7.2);
 *   2. fires `adapter.print(text, kind)` so the line surfaces in the
 *      host's live output (standalone REPL terminal colors / web in-app
 *      console panel).
 *
 * `return` populates a single-slot for the dispatcher to read at script
 * resolve time per SPEC §7.4 (`api.io.return` can be called multiple
 * times; final value wins).
 *
 * `adapter.print` is `MaybePromise<void>` — fire-and-forget here keeps
 * `api.io.print` synchronous on the script side (matches the spec —
 * `print` returns `void`, not `Promise<void>`).
 */
const buildIo = (sink: ScriptIoSink, adapter: PrintingHost): CoreIo => ({
  print: (text: string, kind: PrintKind = 'print') => {
    sink.output.push({ text, kind });
    // Adapters can be sync (standalone) or async (web through workers);
    // either way `print` itself doesn't await. Errors in the host sink
    // shouldn't crash user code — log to the in-memory sink as a fallback
    // so the dispatcher still has the line.
    try {
      void Promise.resolve(adapter.print(text, kind)).catch(() => {
        /* swallow — sink already recorded the line */
      });
    } catch {
      /* swallow */
    }
  },
  return: (value: unknown) => {
    sink.hasReturn = true;
    sink.returnValue = value;
  },
});

/**
 * Minimal slice of CoreAdapter that `buildIo` needs. We don't pull in the
 * full CoreAdapter here so the io wiring is testable in isolation — a
 * dummy `{ print: () => {} }` is enough.
 */
export interface PrintingHost {
  print(text: string, kind: PrintKind): unknown;
}

/**
 * Build the action surface. Every method runs the same three-step gate
 * before dispatching:
 *   1. writes-allowlist check (`validateWriteCapability`)
 *   2. account resolution (`resolveAccount`) + exchange-allowlist check
 *      (`validateExchangeCapability`)
 *   3. `proxy.callWrite(method, args-with-resolved-account)`
 *
 * `runCommand` adds a fourth gate: verb-allowlist + recursion-depth via
 * `validateRunCommandCapability`. Depth is inherited from the dispatch
 * context and carried to the host through the internal proxy envelope.
 */
const buildExchange = (
  proxy: ApiProxy,
  capabilities: Capabilities,
  resolve: (opts: AccountOpts | undefined) => string,
  accountToExchange: Readonly<Record<string, string>>,
  inheritedRunCommandDepth: number,
): CoreExchange => {
  const gateAccount = (account: string): string => {
    const exch = accountToExchange[account];
    if (exch) {
      // Cast is safe — the dispatcher pre-built the map from canonical
      // ExchangeName values via `getAccounts()`.
      validateExchangeCapability(exch as Parameters<typeof validateExchangeCapability>[0], capabilities);
    }
    return account;
  };

  return {
    placeOrder: async (opts) => {
      validateWriteCapability('placeOrder', capabilities);
      const account = gateAccount(resolve(opts));
      return proxy.callWrite('placeOrder', [{ ...opts, account }]);
    },
    placeOrders: async (list) => {
      validateWriteCapability('placeOrders', capabilities);
      // Resolve once — placeOrders is a batch against one account by
      // design. If callers pass mixed `opts.account`s, only the first
      // is honored; the rest are coerced.
      const account = gateAccount(resolve(list[0]));
      const rewritten = list.map((o: PlaceOrderOptions) => ({ ...o, account }));
      return proxy.callWrite('placeOrders', [rewritten]);
    },
    cancelOrder: async (orderId, opts) => {
      validateWriteCapability('cancelOrder', capabilities);
      const account = gateAccount(resolve(opts));
      return proxy.callWrite('cancelOrder', [orderId, { account, symbol: opts?.symbol }]);
    },
    cancelOrders: async (filter, opts) => {
      validateWriteCapability('cancelOrders', capabilities);
      const account = gateAccount(resolve(opts));
      return proxy.callWrite('cancelOrders', [filter, { account }]);
    },
    updateOrder: async (orderId, patch, opts) => {
      validateWriteCapability('updateOrder', capabilities);
      const account = gateAccount(resolve(opts));
      return proxy.callWrite('updateOrder', [orderId, patch, { account }]);
    },
    closePosition: async (opts) => {
      // `closePosition` is synthesized by both real adapters via
      // `placeOrder({ reduceOnly: true })` because safe-cex doesn't
      // expose a per-exchange `closePosition` primitive. The on-the-wire
      // effect is therefore a market order, so we require BOTH
      // capabilities — otherwise a script granted only `closePosition`
      // could still emit `placeOrder` calls through the synthesizer.
      validateWriteCapability('closePosition', capabilities);
      validateWriteCapability('placeOrder', capabilities);
      const account = gateAccount(resolve(opts));
      return proxy.callWrite('closePosition', [{ ...opts, account }]);
    },
    setLeverage: async (symbol, leverage, opts) => {
      validateWriteCapability('setLeverage', capabilities);
      const account = gateAccount(resolve(opts));
      return proxy.callWrite('setLeverage', [symbol, leverage, { account }]);
    },
    setPositionMode: async (mode, opts) => {
      validateWriteCapability('setPositionMode', capabilities);
      const account = gateAccount(resolve(opts));
      return proxy.callWrite('setPositionMode', [mode, { account }]);
    },
    setMarginType: async (symbol, marginType, opts) => {
      validateWriteCapability('setMarginType', capabilities);
      const account = gateAccount(resolve(opts));
      return proxy.callWrite('setMarginType', [symbol, marginType, { account }]);
    },
    runCommand: async (command, opts) => {
      // Two gates: the writes-allowlist (runCommand IS a write method) and
      // the verb-allowlist + depth.
      validateWriteCapability('runCommand', capabilities);
      const depth = inheritedRunCommandDepth + 1;
      validateRunCommandCapability(command, capabilities, depth);
      const account = gateAccount(resolve(opts));
      return proxy.callWrite('runCommand', [command, { account, symbol: opts?.symbol, depth }]);
    },
  };
};

/** Same Phase-2-step-3 stub treatment for storage; Phase 3 wires the impl. */
const buildStorage = (): CoreStorage => {
  const stub = () => {
    throw new Error('api.storage is not wired yet — see SPEC §11 Phase 3');
  };
  return {
    get: stub as CoreStorage['get'],
    set: stub as CoreStorage['set'],
    delete: stub as CoreStorage['delete'],
    list: stub as CoreStorage['list'],
  };
};

/**
 * Assemble the CoreApi the IIFE receives. Read-side getters route through
 * the host's `ApiProxy` after running account-resolution + the reads
 * allowlist; everything else is either a frozen constant table, a pure
 * helper, or a stub (Step 3 / Phase 3).
 */
export const buildApi = (inputs: BuildApiInputs): CoreApi => {
  const { proxy, capabilities, activeAccount, accountToExchange, sink, meta, printingHost } = inputs;

  // Read-side dispatch helper. Three jobs per call:
  //   1. Validate the method against `capabilities.reads`.
  //   2. Resolve the account (when the method takes `opts.account`).
  //   3. Forward to the proxy with the resolved account.
  // The proxy itself enforces structuredClone safety + worker swap-out.
  const resolve = (opts: AccountOpts | undefined): string => resolveAccount(opts, capabilities, activeAccount);

  const api: CoreApi = {
    getActiveAccount: () => {
      validateReadCapability('getActiveAccount', capabilities);
      return proxy.call('getActiveAccount', []);
    },
    getActiveAccounts: () => {
      validateReadCapability('getActiveAccounts', capabilities);
      return proxy.call('getActiveAccounts', []);
    },
    getActiveSymbol: () => {
      validateReadCapability('getActiveSymbol', capabilities);
      return proxy.call('getActiveSymbol', []);
    },
    getActiveExchange: () => {
      validateReadCapability('getActiveExchange', capabilities);
      return proxy.call('getActiveExchange', []);
    },
    getAccounts: () => {
      validateReadCapability('getAccounts', capabilities);
      return proxy.call('getAccounts', []);
    },

    getMarkets: (opts) => {
      validateReadCapability('getMarkets', capabilities);
      return proxy.call('getMarkets', [{ account: resolve(opts) }]);
    },
    getMarket: (symbol, opts) => {
      validateReadCapability('getMarket', capabilities);
      return proxy.call('getMarket', [symbol, { account: resolve(opts) }]);
    },

    getTicker: (symbol, opts) => {
      validateReadCapability('getTicker', capabilities);
      return proxy.call('getTicker', [symbol, { account: resolve(opts) }]);
    },
    getTickers: (symbols, opts) => {
      validateReadCapability('getTickers', capabilities);
      return proxy.call('getTickers', [symbols, { account: resolve(opts) }]);
    },

    getPositions: (opts?: PositionOpts) => {
      validateReadCapability('getPositions', capabilities);
      return proxy.call('getPositions', [{ account: resolve(opts), symbol: opts?.symbol }]);
    },
    getHistoricalPosition: (query: HistoricalPositionQuery) => {
      validateReadCapability('getHistoricalPosition', capabilities);
      // historical-position query is fully-qualified at the call site so
      // resolveAccount isn't applicable; we still gate it against the
      // accounts allowlist for symmetry with the live reads.
      //
      // Snapshot the inputs BEFORE validation, then forward the snapshot —
      // otherwise a `query` whose `account` is a getter (or whose `symbol`
      // is a Proxy) can return a different value on the validate pass vs.
      // the adapter pass, leaking historical data for accounts outside
      // the allowlist.
      const snapshot: HistoricalPositionQuery = {
        account: query.account,
        symbol: query.symbol,
      };
      // Empty allowlist = no restriction (SPEC §2.6) — skip the membership
      // check, same carve-out resolveAccount applies for the live reads.
      if (capabilities.accounts.length > 0 && !capabilities.accounts.includes(snapshot.account)) {
        throw new Error(`account '${snapshot.account}' not in script capabilities`);
      }
      return proxy.call('getHistoricalPosition', [snapshot]);
    },

    getOrders: (opts?: OrderQueryOpts) => {
      validateReadCapability('getOrders', capabilities);
      return proxy.call('getOrders', [{ account: resolve(opts), symbol: opts?.symbol }]);
    },

    getBalance: (opts) => {
      validateReadCapability('getBalance', capabilities);
      return proxy.call('getBalance', [{ account: resolve(opts) }]);
    },
    getCombinedBalance: (opts?: CombinedBalanceOpts) => {
      validateReadCapability('getCombinedBalance', capabilities);
      // Validate every requested account against the script's allowlist —
      // otherwise a script allowlisted for one account could pull balances
      // from arbitrary account names. When `opts.accounts` is omitted,
      // narrow to the allowlist so the dispatcher never reads beyond it.
      //
      // Snapshot opts.accounts via `Array.from` BEFORE validation, then
      // forward the snapshot. A user-supplied iterable (custom
      // `Symbol.iterator` / Proxy / getter) can otherwise yield different
      // values on the validate pass vs. the adapter pass and bypass the
      // allowlist — same TOCTOU class as the `query.account` getter on
      // `getHistoricalPosition`.
      const allowed = capabilities.accounts;
      // Empty allowlist = no restriction (SPEC §2.6): don't validate requested
      // accounts against it, and when none are requested forward `undefined`
      // (NOT `[]`) so the host aggregates ALL streaming accounts — forwarding
      // `[]` would aggregate nothing.
      const unrestricted = allowed.length === 0;
      const requested = opts?.accounts ? Array.from(opts.accounts) : undefined;
      if (requested && !unrestricted) {
        for (const account of requested) {
          if (!allowed.includes(account)) {
            throw new Error(`account '${account}' not in script capabilities`);
          }
        }
      }
      return proxy.call('getCombinedBalance', [{ accounts: requested ?? (unrestricted ? undefined : allowed) }]);
    },

    getOrderbook: (symbol: string, opts?: OrderbookOpts) => {
      validateReadCapability('getOrderbook', capabilities);
      return proxy.call('getOrderbook', [symbol, { account: resolve(opts), depth: opts?.depth }]);
    },
    getCandles: (symbol: string, timeframe: Timeframe, opts) => {
      validateReadCapability('getCandles', capabilities);
      return proxy.call('getCandles', [
        symbol,
        timeframe,
        { account: resolve(opts), from: opts?.from, to: opts?.to, limit: opts?.limit },
      ]);
    },

    exchange: buildExchange(proxy, capabilities, resolve, accountToExchange, inputs.runCommandDepth ?? 0),
    io: buildIo(sink, printingHost),
    storage: buildStorage(),
    utils: CORE_UTILS,
    constants: CORE_CONSTANTS,
    version: CORE_API_VERSION,
    meta,
  };

  return api;
};

/** Re-export for callers that want the type alongside the value. */
export type { InvokedFrom };
