/**
 * Standalone CLI CoreAdapter — wires `@tealstreet/script-api-core`'s full
 * surface (read + action + io + runCommand) to the in-process
 * `ExchangeManager` + `CLISession` + cli-core's `executeCommand` the REPL
 * already owns. See SPEC §11 Phase 1 + Phase 2.
 *
 * Contract held with `script-api-core`:
 *   - Methods take already-resolved accounts; the dispatcher ran
 *     `resolveAccount` + the capability gates BEFORE this layer.
 *   - The adapter is dumb: no re-validation, no capability checks.
 *   - Sync where convenient (reads off the store map), async where the
 *     underlying call is async (writes hit safe-cex's `BaseExchange`).
 *
 * Read-side: reads come straight off
 * `exchange.store.{markets,tickers,positions,orders,balance}`, populated
 * by safe-cex + the running WS feeds.
 *
 * Write-side: dispatches through the live `BaseExchange` instance carried
 * on `ExchangeManager.get(account).exchange`. The exchange handles
 * signing transparently (local in-process or remote via
 * `WebSocketRemoteSigner` — see `apps/cli/CLAUDE.md` → "Remote Signing"),
 * so `signer` here stays `undefined` until Phase 8's worker boundary.
 *
 * `runCommand` routes through cli-core's `executeCommand`, not the
 * standalone `repl.ts` wrapper. The dispatcher already gated the verb
 * against the script's `runCommand.allowedVerbs`, so by the time we
 * reach `executeCommand` the verb is allowlisted; if cli-core still
 * returns `handled:false`, surface that as an unknown-command failure
 * rather than silent success.
 *
 * `print(text, kind)` routes through the platform-adapter's kinded
 * print methods. The platform-adapter's `AsyncLocalStorage` sink (see
 * `apps/cli/src/listener/adapter-context.ts`) transparently routes to
 * terminal+log OR listener WS frames depending on invocation context.
 *
 * `getHistoricalPosition` and `getOrderbook` still return undefined — the
 * standalone CLI has no historical-position store and no main-thread
 * orderbook snapshot today; Phase 3+ wires them when those stores land.
 */
import { executeCommand, type CLIPlatformAdapter, type CLISession, type ExchangeLike } from '@tealstreet/cli-core';
import type {
  Balance,
  Candle,
  ExchangeName,
  MarginType,
  Market,
  Order,
  OrderBook,
  PlaceOrderOpts,
  Position,
  PositionMode,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from '@tealstreet/safe-cex/src/types';
import type {
  AccountInfo,
  CancelFilter,
  ClosePositionOpts,
  CombinedBalance,
  CoreAdapter,
  HistoricalPosition,
  PlaceOrderOptions,
  PrintKind,
  RunCommandResult,
} from '@tealstreet/script-api-core';

import type { ExchangeManager } from './exchange-manager';
import { recordScriptWrite } from './audit-log';

const EMPTY: readonly never[] = Object.freeze([]);

const storeOf = (exchange: ExchangeLike) => exchange.store ?? {};

const summarizeBalance = (
  exchangeManager: ExchangeManager,
  accounts: readonly string[] | undefined,
): CombinedBalance => {
  const names = accounts ?? exchangeManager.getConnectedAccountNames();
  let totalEquity = 0;
  let totalAvailable = 0;
  let totalUsed = 0;
  let totalUpnl = 0;
  let longExposure = 0;
  let shortExposure = 0;
  for (const name of names) {
    const running = exchangeManager.get(name);
    if (!running) continue;
    const store = storeOf(running.exchange);
    const bal = store.balance;
    if (bal) {
      totalEquity += bal.total ?? 0;
      totalAvailable += bal.free ?? 0;
      totalUsed += bal.used ?? 0;
      totalUpnl += bal.upnl ?? 0;
    }
    for (const pos of store.positions ?? []) {
      if (pos.side === 'long') longExposure += pos.notional;
      else if (pos.side === 'short') shortExposure += pos.notional;
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

const toSafeCexPlaceOrder = (opts: Omit<PlaceOrderOptions, 'account'>): PlaceOrderOpts =>
  ({
    ...opts,
  }) as unknown as PlaceOrderOpts;

const requireExchange = (exchangeManager: ExchangeManager, account: string): ExchangeLike => {
  const exchange = exchangeManager.getExchange(account);
  if (!exchange) {
    throw new Error(`exchange not connected for account '${account}'`);
  }
  return exchange;
};

/**
 * Narrow the optional `ExchangeLike` action method to a callable. The
 * runtime type is `BaseExchange` for every connected account, so all
 * methods exist — but `ExchangeLike` types them defensively as optional.
 * This helper throws with a clear message when an exchange genuinely
 * doesn't expose the action (rare, e.g. read-only feeds).
 */
const requireMethod = <K extends keyof ExchangeLike>(
  exchange: ExchangeLike,
  method: K,
  account: string,
): NonNullable<ExchangeLike[K]> => {
  const fn = exchange[method];
  if (!fn) {
    throw new Error(`account '${account}' does not support ${String(method)}`);
  }
  return fn as NonNullable<ExchangeLike[K]>;
};

/** Map `PrintKind` onto the existing `CLIPlatformAdapter.printX` family. */
const routePrint = (adapter: CLIPlatformAdapter, text: string, kind: PrintKind): void => {
  switch (kind) {
    case 'success':
      adapter.printSuccess(text);
      return;
    case 'info':
      adapter.printInfo(text);
      return;
    case 'warn':
      adapter.printWarning(text);
      return;
    case 'error':
      adapter.printError(text);
      return;
    case 'print':
    default:
      adapter.print([text]);
      return;
  }
};

/**
 * Build a `CoreAdapter` for the standalone CLI runtime.
 *
 * `platformAdapter` is optional only because Phase 1's call sites don't
 * carry it yet — Phase 2's host shim (`apps/cli/src/script-runner.ts`)
 * always passes it. When absent, `print` / `runCommand` throw with a
 * clear error so we catch wiring gaps early instead of silently no-oping.
 */
export const createStandaloneCoreAdapter = (
  exchangeManager: ExchangeManager,
  session: CLISession,
  platformAdapter?: CLIPlatformAdapter,
): CoreAdapter => {
  const requireAdapter = (): CLIPlatformAdapter => {
    if (!platformAdapter) {
      throw new Error(
        'standalone CoreAdapter built without CLIPlatformAdapter — pass one before invoking print/runCommand',
      );
    }
    return platformAdapter;
  };

  return {
    getActiveAccount: () => session.currentAccountName ?? '',
    getActiveAccounts: () => exchangeManager.getConnectedAccountNames(),
    getActiveSymbol: () => session.currentSymbol,
    getActiveExchange: () => {
      const name = session.currentAccountName;
      const running = name ? exchangeManager.get(name) : undefined;
      const cfg = running?.accountConfig;
      return (cfg?.exchange?.toLowerCase() ?? 'bybit') as ExchangeName;
    },
    getAccounts: (): readonly AccountInfo[] => {
      return exchangeManager.getAll().map((running) => ({
        name: running.accountName,
        exchange: running.accountConfig.exchange.toLowerCase() as ExchangeName,
        isStreaming: true,
      }));
    },

    getMarkets: (account): readonly Market[] => {
      const exchange = exchangeManager.getExchange(account);
      return exchange ? (storeOf(exchange).markets ?? EMPTY) : EMPTY;
    },
    getMarket: (symbol, account): Market | undefined => {
      const exchange = exchangeManager.getExchange(account);
      return exchange ? storeOf(exchange).markets?.find((m) => m.symbol === symbol) : undefined;
    },

    getTicker: (symbol, account): Ticker | undefined => {
      const exchange = exchangeManager.getExchange(account);
      return exchange ? storeOf(exchange).tickers?.find((t) => t.symbol === symbol) : undefined;
    },
    getTickers: (symbols, account): readonly Ticker[] => {
      const exchange = exchangeManager.getExchange(account);
      if (!exchange) return EMPTY;
      const want = new Set(symbols);
      return storeOf(exchange).tickers?.filter((t) => want.has(t.symbol)) ?? EMPTY;
    },

    getPositions: ({ account, symbol }): readonly Position[] => {
      const exchange = exchangeManager.getExchange(account);
      if (!exchange) return EMPTY;
      const list = storeOf(exchange).positions ?? EMPTY;
      return symbol ? list.filter((p) => p.symbol === symbol) : list;
    },
    getHistoricalPosition: (): HistoricalPosition | undefined => {
      // TODO Phase 3+: wire historical-position source into the standalone store.
      return undefined;
    },

    getOrders: ({ account, symbol }): readonly Order[] => {
      const exchange = exchangeManager.getExchange(account);
      if (!exchange) return EMPTY;
      const list = storeOf(exchange).orders ?? EMPTY;
      return symbol ? list.filter((o) => o.symbol === symbol) : list;
    },

    getBalance: (account): Balance | undefined => {
      const exchange = exchangeManager.getExchange(account);
      return exchange ? storeOf(exchange).balance : undefined;
    },
    getCombinedBalance: ({ accounts }): CombinedBalance => summarizeBalance(exchangeManager, accounts),

    getOrderbook: (): OrderBook | undefined => {
      // TODO Phase 3+: wire wsPublic OB feed into the standalone store; for
      // now scripts that need OB depth fall back to `getTicker` quotes.
      return undefined;
    },

    getCandles: async (
      symbol: string,
      timeframe: Timeframe,
      { account, from, to, limit }: { account: string; from?: number; to?: number; limit?: number },
    ): Promise<readonly Candle[]> => {
      const exchange = exchangeManager.getExchange(account);
      if (!exchange?.fetchOHLCV) return EMPTY;
      const candles = await exchange.fetchOHLCV({
        symbol,
        interval: timeframe,
        from,
        to,
        limit,
      });
      return candles ?? EMPTY;
    },

    // ── Phase 2 writes — route through the live BaseExchange instance ─────
    placeOrder: async (opts, account) => {
      const exchange = requireExchange(exchangeManager, account);
      const placeOrder = requireMethod(exchange, 'placeOrder', account);
      const summary = `placeOrder ${opts.symbol} ${opts.side} ${opts.amount}`;
      try {
        const orderIds = await placeOrder(toSafeCexPlaceOrder(opts));
        recordScriptWrite(account, summary, { orderIds: orderIds.slice() });
        return { orderIds: Object.freeze(orderIds.slice()) };
      } catch (e) {
        recordScriptWrite(account, summary, { ok: false, error: (e as Error).message });
        throw e;
      }
    },
    placeOrders: async (orders, account) => {
      const exchange = requireExchange(exchangeManager, account);
      const placeOrders = requireMethod(exchange, 'placeOrders', account);
      const summary = `placeOrders ${orders.length}× ${orders[0]?.symbol ?? ''}`.trim();
      try {
        const orderIds = await placeOrders(orders.map((o) => toSafeCexPlaceOrder(o)));
        recordScriptWrite(account, summary, { orderIds: orderIds.slice() });
        return { orderIds: Object.freeze(orderIds.slice()) };
      } catch (e) {
        recordScriptWrite(account, summary, { ok: false, error: (e as Error).message });
        throw e;
      }
    },
    cancelOrder: async (orderId, { account, symbol }) => {
      const exchange = requireExchange(exchangeManager, account);
      const cancelOrders = requireMethod(exchange, 'cancelOrders', account);
      const summary = `cancelOrder ${orderId}`;
      try {
        const order = (storeOf(exchange).orders ?? []).find(
          (o) => o.id === orderId && (symbol === undefined || o.symbol === symbol),
        );
        if (!order) {
          throw new Error(`cancelOrder: no order with id '${orderId}' on account '${account}'`);
        }
        await cancelOrders([order]);
        recordScriptWrite(account, summary);
      } catch (e) {
        recordScriptWrite(account, summary, { ok: false, error: (e as Error).message });
        throw e;
      }
    },
    cancelOrders: async (filter: CancelFilter, account) => {
      const exchange = requireExchange(exchangeManager, account);
      const cancelOrders = requireMethod(exchange, 'cancelOrders', account);
      let candidates = (storeOf(exchange).orders ?? []).slice();
      if (filter.symbol !== undefined) candidates = candidates.filter((o) => o.symbol === filter.symbol);
      if (filter.side !== undefined) candidates = candidates.filter((o) => o.side === filter.side);
      if (filter.reduceOnly !== undefined) {
        const want = Boolean(filter.reduceOnly);
        candidates = candidates.filter((o) => Boolean(o.reduceOnly) === want);
      }
      if (candidates.length === 0) return { canceled: 0 };
      const summary = `cancelOrders ${candidates.length}× ${filter.symbol ?? ''}`.trim();
      try {
        await cancelOrders(candidates);
        recordScriptWrite(account, summary);
      } catch (e) {
        recordScriptWrite(account, summary, { ok: false, error: (e as Error).message });
        throw e;
      }
      return { canceled: candidates.length };
    },
    updateOrder: async (orderId, patch: UpdateOrderOpts, account) => {
      const exchange = requireExchange(exchangeManager, account);
      const updateOrder = requireMethod(exchange, 'updateOrder', account);
      const summary = `updateOrder ${orderId}`;
      try {
        const order = (storeOf(exchange).orders ?? []).find((o) => o.id === orderId);
        if (!order) {
          throw new Error(`updateOrder: no order with id '${orderId}' on account '${account}'`);
        }
        await updateOrder({ ...patch, order } as UpdateOrderOpts);
        recordScriptWrite(account, summary);
      } catch (e) {
        recordScriptWrite(account, summary, { ok: false, error: (e as Error).message });
        throw e;
      }
    },
    closePosition: async (opts: Omit<ClosePositionOpts, 'account'>, account) => {
      const exchange = requireExchange(exchangeManager, account);
      const placeOrder = requireMethod(exchange, 'placeOrder', account);
      // safe-cex doesn't expose `closePosition` as a primitive on every
      // exchange — synthesize via a reduce-only market against the
      // matching open positions. No matching position → empty orderIds.
      const candidates = (storeOf(exchange).positions ?? []).filter(
        (p) => p.symbol === opts.symbol && (!opts.side || p.side === opts.side),
      );
      if (candidates.length === 0) {
        return { orderIds: Object.freeze([] as readonly string[]) };
      }
      const summary = `closePosition ${opts.symbol}${opts.side ? ' ' + opts.side : ''}`;
      // Accumulate outside the try so a mid-loop throw still audits the
      // partial fills already placed (ok:false + the IDs we got).
      const orderIds: string[] = [];
      try {
        for (const position of candidates) {
          const closeSide = position.side === 'long' ? 'sell' : 'buy';
          const ids = await placeOrder({
            symbol: opts.symbol,
            side: closeSide,
            type: 'market',
            amount: opts.amount ?? position.contracts,
            reduceOnly: true,
          } as PlaceOrderOpts);
          for (const id of ids ?? []) orderIds.push(id);
        }
        recordScriptWrite(account, summary, { orderIds });
        return { orderIds: Object.freeze(orderIds) };
      } catch (e) {
        recordScriptWrite(account, summary, { ok: false, error: (e as Error).message, orderIds });
        throw e;
      }
    },
    setLeverage: async (symbol, leverage, account) => {
      const exchange = requireExchange(exchangeManager, account);
      const setLeverage = requireMethod(exchange, 'setLeverage', account);
      const summary = `setLeverage ${symbol} ${leverage}`;
      try {
        await setLeverage(symbol, leverage);
        recordScriptWrite(account, summary);
      } catch (e) {
        recordScriptWrite(account, summary, { ok: false, error: (e as Error).message });
        throw e;
      }
    },
    setPositionMode: async (mode: PositionMode, account) => {
      const exchange = requireExchange(exchangeManager, account);
      // safe-cex's lifecycle uses `changePositionMode(hedged, symbol?)`;
      // cli-core's `ExchangeLike` doesn't surface it, so narrow to the
      // BaseExchange runtime shape via a typed cast.
      const change = (
        exchange as unknown as {
          changePositionMode?: (hedged: boolean, symbol?: string) => Promise<void>;
        }
      ).changePositionMode;
      if (!change) {
        throw new Error(`setPositionMode: account '${account}' does not support position-mode changes`);
      }
      await change(mode === 'hedged');
    },
    setMarginType: async (symbol, marginType: MarginType, account) => {
      const exchange = requireExchange(exchangeManager, account);
      const change = (
        exchange as unknown as {
          changeMarginType?: (isolated: boolean, symbol?: string) => Promise<void>;
        }
      ).changeMarginType;
      if (!change) {
        throw new Error(`setMarginType: account '${account}' does not support margin-type changes`);
      }
      await change(marginType === 'isolated', symbol);
    },

    // ── Phase 2 runCommand — verb-allowlist already enforced by dispatcher ──
    runCommand: async (command, { account, symbol }): Promise<RunCommandResult> => {
      const platform = requireAdapter();
      const exchange = exchangeManager.getExchange(account) ?? null;
      const sym = symbol ?? session.currentSymbol;
      try {
        const result = await executeCommand(command, sym, account, exchange, platform);
        if (!result.handled) {
          const verb = command.trim().split(/\s+/)[0] ?? '';
          return {
            ok: false,
            capturedIds: [],
            output: [],
            error: verb ? `Unknown command: ${verb}` : 'Unknown command',
          };
        }
        const success = result.success !== false;
        return {
          ok: success,
          capturedIds: result.data?.orderIds ?? [],
          // executeCommand prints via the platform adapter directly; the
          // sink isn't returned to us here. Scripts that want the printed
          // transcript still see lines via `api.io.print` — just not
          // bundled into this RunCommandResult.
          output: [],
          error: success ? undefined : result.error,
        };
      } catch (err) {
        return {
          ok: false,
          capturedIds: [],
          output: [],
          error: err instanceof Error ? err.message : String(err),
        };
      }
    },

    // ── Phase 2 print — route through CLIPlatformAdapter (ALS-aware) ────────
    print: (text, kind) => {
      routePrint(requireAdapter(), text, kind);
    },

    signer: undefined,
  };
};
