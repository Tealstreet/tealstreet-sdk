/**
 * MockCoreAdapter — a fixture-driven CoreAdapter for tests. Same factory feeds
 * the parity test (used as both "standalone-like" and "web-like" instances,
 * verifying that two adapters with identical seed produce identical proxy
 * results). Pure data — no real exchange wiring.
 *
 * Phase 2 adds a writeLog + ioLog + commandLog so action tests can assert
 * "the dispatcher actually called placeOrder" without spinning up safe-cex.
 */
import type {
  Balance,
  Candle,
  ExchangeName,
  MarginType,
  Market,
  Order,
  OrderBook,
  Position,
  PositionMode,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from '@tealstreet/safe-cex/src/types';

import type { CoreAdapter } from '../adapter/AdapterContext';
import type {
  AccountInfo,
  CancelFilter,
  ClosePositionOpts,
  CombinedBalance,
  HistoricalPosition,
  PlaceOrderOptions,
  PrintKind,
  RunCommandResult,
} from '../api/CoreApi.types';

export interface MockSeed {
  readonly activeAccount: string;
  readonly activeSymbol: string;
  readonly activeExchange: ExchangeName;
  readonly streamingAccounts: readonly string[];
  readonly accounts: readonly AccountInfo[];
  readonly markets: Readonly<Record<string, readonly Market[]>>;
  readonly tickers: Readonly<Record<string, readonly Ticker[]>>;
  readonly positions: Readonly<Record<string, readonly Position[]>>;
  readonly orders: Readonly<Record<string, readonly Order[]>>;
  readonly balance: Readonly<Record<string, Balance>>;
  readonly historical?: readonly HistoricalPosition[];
  /** Keyed `${account}::${symbol}`. */
  readonly orderbook?: Readonly<Record<string, OrderBook>>;
  /** Keyed `${account}::${symbol}::${timeframe}`. */
  readonly candles?: Readonly<Record<string, readonly Candle[]>>;
}

/**
 * Build the CombinedBalance shape from per-account `Balance` rows. The host
 * adapters (apps/cli, apps/web) will roll their own — this one summarizes from
 * the seed so the parity test has a deterministic value to compare against.
 */
const buildCombinedBalance = (seed: MockSeed, filterAccounts?: readonly string[]): CombinedBalance => {
  const accounts = filterAccounts ?? seed.streamingAccounts;
  let totalEquity = 0;
  let totalAvailable = 0;
  let totalUsed = 0;
  let totalUpnl = 0;
  let longExposure = 0;
  let shortExposure = 0;
  for (const acct of accounts) {
    const bal = seed.balance[acct];
    if (bal) {
      totalEquity += bal.total ?? 0;
      totalAvailable += bal.free ?? 0;
      totalUsed += bal.used ?? 0;
      totalUpnl += bal.upnl ?? 0;
    }
    for (const pos of seed.positions[acct] ?? []) {
      if (pos.side === 'long') longExposure += pos.notional;
      else shortExposure += pos.notional;
    }
  }
  return {
    totalEquity,
    totalAvailable,
    totalUsed,
    totalUpnl,
    longExposure,
    shortExposure,
    currency: 'USD',
  };
};

/** Discriminated union of write-side calls so tests can pattern-match. */
export type MockWriteCall =
  | { kind: 'placeOrder'; opts: Omit<PlaceOrderOptions, 'account'>; account: string }
  | {
      kind: 'placeOrders';
      list: ReadonlyArray<Omit<PlaceOrderOptions, 'account'>>;
      account: string;
    }
  | {
      kind: 'cancelOrder';
      orderId: string;
      opts: { account: string; symbol?: string };
    }
  | { kind: 'cancelOrders'; filter: CancelFilter; account: string }
  | {
      kind: 'updateOrder';
      orderId: string;
      patch: UpdateOrderOpts;
      account: string;
    }
  | {
      kind: 'closePosition';
      opts: Omit<ClosePositionOpts, 'account'>;
      account: string;
    }
  | { kind: 'setLeverage'; symbol: string; leverage: number; account: string }
  | { kind: 'setPositionMode'; mode: PositionMode; account: string }
  | {
      kind: 'setMarginType';
      symbol: string;
      marginType: MarginType;
      account: string;
    };

export interface MockRunCommandInvocation {
  readonly command: string;
  readonly account: string;
  readonly symbol?: string;
  readonly depth: number;
}

export interface MockAdapterLogs {
  /** Every action call routed through the proxy. */
  readonly writes: MockWriteCall[];
  /** Every `api.io.print` line that reached the adapter (Phase 2 step 4 wiring). */
  readonly io: { text: string; kind: PrintKind }[];
  /** Every `api.exchange.runCommand` dispatch with its threaded depth. */
  readonly commands: MockRunCommandInvocation[];
}

export interface MockAdapter extends CoreAdapter {
  /** Test-only handle to inspect what the dispatcher routed through. */
  readonly logs: MockAdapterLogs;
}

let mockOrderIdCounter = 0;
const nextOrderId = () => `mock-order-${++mockOrderIdCounter}`;

export const createMockAdapter = (seed: MockSeed): MockAdapter => {
  const findHistorical = (account: string, symbol: string) =>
    seed.historical?.find((h) => h.account === account && h.symbol === symbol);

  const logs: MockAdapterLogs = {
    writes: [],
    io: [],
    commands: [],
  };

  return {
    getActiveAccount: () => seed.activeAccount,
    getActiveAccounts: () => seed.streamingAccounts,
    getActiveSymbol: () => seed.activeSymbol,
    getActiveExchange: () => seed.activeExchange,
    getAccounts: () => seed.accounts,

    getMarkets: (account) => seed.markets[account] ?? [],
    getMarket: (symbol, account) => (seed.markets[account] ?? []).find((m) => m.symbol === symbol),

    getTicker: (symbol, account) => (seed.tickers[account] ?? []).find((t) => t.symbol === symbol),
    getTickers: (symbols, account) => {
      const list = seed.tickers[account] ?? [];
      const want = new Set(symbols);
      return list.filter((t) => want.has(t.symbol));
    },

    getPositions: ({ account, symbol }) => {
      const list = seed.positions[account] ?? [];
      return symbol ? list.filter((p) => p.symbol === symbol) : list;
    },
    getHistoricalPosition: ({ account, symbol }) => findHistorical(account, symbol),

    getOrders: ({ account, symbol }) => {
      const list = seed.orders[account] ?? [];
      return symbol ? list.filter((o) => o.symbol === symbol) : list;
    },

    getBalance: (account) => seed.balance[account],
    getCombinedBalance: ({ accounts }) => buildCombinedBalance(seed, accounts),

    getOrderbook: (symbol, { account }) => seed.orderbook?.[`${account}::${symbol}`],

    getCandles: (symbol: string, timeframe: Timeframe, { account, limit }) => {
      const list = seed.candles?.[`${account}::${symbol}::${timeframe}`] ?? [];
      return limit && limit < list.length ? list.slice(-limit) : list;
    },

    // ── Write surface — no real exchange; record the call for assertions. ──
    placeOrder: (opts, account) => {
      logs.writes.push({ kind: 'placeOrder', opts, account });
      return { orderIds: [nextOrderId()] };
    },
    placeOrders: (list, account) => {
      logs.writes.push({ kind: 'placeOrders', list, account });
      return { orderIds: list.map(() => nextOrderId()) };
    },
    cancelOrder: (orderId, opts) => {
      logs.writes.push({ kind: 'cancelOrder', orderId, opts });
    },
    cancelOrders: (filter, account) => {
      logs.writes.push({ kind: 'cancelOrders', filter, account });
      return { canceled: 0 };
    },
    updateOrder: (orderId, patch, account) => {
      logs.writes.push({ kind: 'updateOrder', orderId, patch, account });
    },
    closePosition: (opts, account) => {
      logs.writes.push({ kind: 'closePosition', opts, account });
      return { orderIds: [nextOrderId()] };
    },
    setLeverage: (symbol, leverage, account) => {
      logs.writes.push({ kind: 'setLeverage', symbol, leverage, account });
    },
    setPositionMode: (mode, account) => {
      logs.writes.push({ kind: 'setPositionMode', mode, account });
    },
    setMarginType: (symbol, marginType, account) => {
      logs.writes.push({ kind: 'setMarginType', symbol, marginType, account });
    },

    runCommand: (command, { account, symbol, depth }): RunCommandResult => {
      logs.commands.push({ command, account, symbol, depth });
      return { ok: true, capturedIds: [], output: [] };
    },

    print: (text, kind) => {
      logs.io.push({ text, kind });
    },

    signer: undefined,
    logs,
  };
};
