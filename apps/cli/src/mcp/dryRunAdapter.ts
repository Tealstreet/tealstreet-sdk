import type { CoreAdapter } from '@tealstreet/script-api-core';

/**
 * A `CoreAdapter` that answers reads from live data but RECORDS writes instead
 * of executing them.
 *
 * ## Why a recording wrapper and not restrictive capabilities
 *
 * The obvious alternative is the `FOOTER_READONLY_CAPS` mechanism (see
 * `apps/cli/CLAUDE.md`): hand the script `writes: []` so `capabilityCheck`
 * refuses every write. That is the right tool for the footer, but it is the
 * WRONG tool here — it *throws on the first write and aborts the script*, so
 * you learn about exactly one intended action and nothing about the rest. The
 * entire point of a dry run is the full list of what a script would do.
 *
 * ## Every member is enumerated explicitly. Do not refactor to a spread.
 *
 * `{ ...realAdapter, placeOrder: stub }` is deny-by-omission: the day someone
 * adds a new write method to `CoreAdapter`, it silently passes through to live
 * trading in what the user was told is a dry run. Listing every member means a
 * new one fails to compile here instead.
 *
 * ## What this does NOT protect against
 *
 * Scripts execute with full Bun globals. A script that reads `config.json` and
 * places an order out-of-band is unaffected by any of this. Dry run bounds
 * HONEST scripts; it is not a sandbox. That is why callers must still gate on
 * `validate()` + `checkTopLevelEffects` before running anything.
 */

export interface DryRunIntent {
  method: string;
  account: string;
  detail: string;
}

const describe = (value: unknown): string => {
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
};

export const createDryRunAdapter = (
  real: CoreAdapter,
): { adapter: CoreAdapter; intents: DryRunIntent[]; output: string[] } => {
  const intents: DryRunIntent[] = [];
  const output: string[] = [];

  const record = (method: string, account: string, detail: unknown): void => {
    intents.push({ method, account, detail: describe(detail) });
  };

  const adapter: CoreAdapter = {
    // ── Reads: pass through to live data ────────────────────────────────
    // getCandles is the only one with an outbound REST call; it changes no
    // state, so it stays live — a dry run against stale candles would be
    // misleading in a way that defeats the purpose.
    getActiveAccount: () => real.getActiveAccount(),
    getActiveAccounts: () => real.getActiveAccounts(),
    getActiveSymbol: () => real.getActiveSymbol(),
    getActiveExchange: () => real.getActiveExchange(),
    getAccounts: () => real.getAccounts(),
    getMarkets: (account) => real.getMarkets(account),
    getMarket: (symbol, account) => real.getMarket(symbol, account),
    getTicker: (symbol, account) => real.getTicker(symbol, account),
    getTickers: (symbols, account) => real.getTickers(symbols, account),
    getPositions: (opts) => real.getPositions(opts),
    getHistoricalPosition: (opts) => real.getHistoricalPosition(opts),
    getOrders: (opts) => real.getOrders(opts),
    getBalance: (account) => real.getBalance(account),
    getCombinedBalance: (opts) => real.getCombinedBalance(opts),
    getOrderbook: (symbol, opts) => real.getOrderbook(symbol, opts),
    getCandles: (symbol, timeframe, opts) => real.getCandles(symbol, timeframe, opts),

    // ── Writes: recorded, never executed ────────────────────────────────
    placeOrder: async (opts, account) => {
      record('placeOrder', account, opts);
      return { orderIds: Object.freeze([`dry-run-${intents.length}`]) };
    },
    placeOrders: async (orders, account) => {
      record('placeOrders', account, orders);
      return { orderIds: Object.freeze(orders.map((_, i) => `dry-run-${intents.length}-${i}`)) };
    },
    cancelOrder: async (orderId, opts) => {
      record('cancelOrder', opts.account, { orderId, symbol: opts.symbol });
    },
    cancelOrders: async (filter, account) => {
      record('cancelOrders', account, filter);
      return { canceled: 0 };
    },
    updateOrder: async (orderId, patch, account) => {
      record('updateOrder', account, { orderId, patch });
    },
    closePosition: async (opts, account) => {
      record('closePosition', account, opts);
      return { orderIds: Object.freeze([`dry-run-${intents.length}`]) };
    },
    setLeverage: async (symbol, leverage, account) => {
      record('setLeverage', account, { symbol, leverage });
    },
    setPositionMode: async (mode, account) => {
      record('setPositionMode', account, { mode });
    },
    setMarginType: async (symbol, marginType, account) => {
      record('setMarginType', account, { symbol, marginType });
    },

    // `runCommand` dispatches arbitrary REPL commands — including `buy` and
    // `sell` — so it is the single most important thing to stub.
    //
    // It returns ok:true with synthetic ids on purpose. Returning a failure
    // would push a correctly-written script down its error branch, so the dry
    // run would report what the script does when things break rather than what
    // it does normally — the opposite of what was asked.
    runCommand: async (command, opts) => {
      record('runCommand', opts.account ?? 'unknown', { command });
      return {
        ok: true,
        capturedIds: Object.freeze([`dry-run-cmd-${intents.length}`]),
        output: Object.freeze([{ text: `[dry-run] ${command}`, kind: 'info' as const }]),
      };
    },

    print: (text) => {
      output.push(text);
    },

    signer: undefined,
  };

  return { adapter, intents, output };
};
