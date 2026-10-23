import type {
  Balance,
  Candle,
  ExchangeName,
  Market,
  Order,
  PlaceOrderOpts,
  Position,
  Ticker,
  UpdateOrderOpts,
} from '@tealstreet/safe-cex/src/types';

import type {
  AccountInfo,
  CancelFilter,
  ClosePositionOpts,
  CombinedBalance,
  HistoricalPosition,
  PlaceOrderOptions,
  PrintKind,
  RunCommandResult,
  UpdateOrderPatch,
} from '../api/CoreApi.types';
import type { CoreAdapter } from './AdapterContext';
import type { HostBindings, HostExchange, HostWriteEvent } from './HostBindings';

import { clampReduceOnlyOrder, prepareOrder } from '../order/prepareOrder';
import {
  assertMinimumPrice,
  roundMarketCloseSize,
  roundOrderPrice,
  roundOrderSizeToNearest,
} from '../order/orderPricing';
import { nativePositionSize } from '../order/positionSizing';
import { ScriptValidationError } from '../order/ScriptValidationError';
import { bySymbol } from './HostBindings';
import { ScriptBatchOrderError } from './ScriptBatchOrderError';

/**
 * The `CoreAdapter` body — written ONCE, for every runtime.
 *
 * Web, CLI and mobile each used to hand-roll this. They shared a contract and
 * nothing else, so they drifted where nobody was looking: the CLI applied no
 * rounding and no minimum-size validation, only web ran the reduce-only clamp,
 * and mobile threw on every write. Three implementations of one interface is
 * three chances to be wrong about the same thing.
 *
 * Everything host-specific arrives through `HostBindings`. If you find
 * yourself adding a `if (host.kind === 'web')` here, the thing you are
 * branching on is a missing binding.
 *
 * ## What this deliberately does NOT do
 *
 * No toasts, no order beep, no optimistic order ids, no fat-finger modal.
 * Those live in `apps/web/src/order/*` and belong to a human clicking a
 * button — fat-finger in particular would block a headless script behind a
 * modal that nothing is there to dismiss. This layer owns the NUMBERS.
 */

const EMPTY: readonly never[] = Object.freeze([]);

const lower = (name: string | undefined, fallback: ExchangeName): ExchangeName =>
  ((name ?? fallback) as string).toLowerCase() as ExchangeName;

export const createCoreAdapter = (host: HostBindings): CoreAdapter => {
  const requireExchange = (account: string): HostExchange => {
    const exchange = host.getExchange(account);
    if (!exchange) throw new Error(`exchange not connected for account '${account}'`);
    return exchange;
  };

  const requireMethod = <K extends keyof HostExchange>(
    exchange: HostExchange,
    method: K,
    account: string,
  ): NonNullable<HostExchange[K]> => {
    const fn = exchange[method];
    if (!fn) throw new Error(`account '${account}' does not support ${String(method)}`);
    return fn as NonNullable<HostExchange[K]>;
  };

  const findMarket = (symbol: string, account: string): Market | undefined =>
    host.findMarket
      ? host.findMarket(symbol, account)
      : host.getStore(account)?.markets?.find((market) => market.symbol === symbol);

  const requireMarket = (symbol: string, account: string, cache?: Map<string, Market>): Market => {
    const cached = cache?.get(symbol);
    if (cached) return cached;
    const market = findMarket(symbol, account);
    if (!market) throw new ScriptValidationError(`unknown market '${symbol}' on account '${account}'`);
    cache?.set(symbol, market);
    return market;
  };

  const findTicker = (symbol: string, account: string): Ticker | undefined =>
    host.findTicker
      ? host.findTicker(symbol, account)
      : host.getStore(account)?.tickers?.find((ticker) => ticker.symbol === symbol);

  /**
   * The order policy is `order/prepareOrder.ts`, not this closure. It used to
   * live here, which made it reachable only by holding an adapter — and the
   * CLI's trading commands cannot hold one (they run from three hosts, two of
   * which build a `CommandContext` with a bare exchange handle and no
   * account). So they wrote to the venue unrounded. These two wrappers supply
   * the host-shaped inputs and nothing else.
   */
  const toSafeCexPlaceOrder = (
    opts: Omit<PlaceOrderOptions, 'account'>,
    account: string,
    cache?: Map<string, Market>,
  ): PlaceOrderOpts =>
    prepareOrder(opts, {
      market: requireMarket(opts.symbol, account, cache),
      exchangeName: host.getExchange(account)?.name,
    });

  const clampReduceOnly = (order: PlaceOrderOpts, account: string, market: Market): PlaceOrderOpts => {
    const exchange = host.getExchange(account);
    return clampReduceOnlyOrder(order, {
      market,
      exchangeName: exchange?.name,
      // `getStore` first: the CLI's exchange handle and a mobile bridge do not
      // both carry a `store`, and an empty position list clamps to nothing at
      // all rather than failing loudly.
      positions: (host.getStore(account)?.positions ?? exchange?.store?.positions ?? EMPTY) as Position[],
      // A binding because web holds OPTIMISTIC in-progress rows that exist
      // nowhere else. A host without it clamps against the store, which is
      // correct — just less current.
      restingOrders: host.readRestingOrders?.(account) ?? host.getStore(account)?.orders ?? EMPTY,
      ticker: findTicker(order.symbol, account),
    });
  };

  /** Every write calls this first. A host that declines writes declines ALL
   *  of them, including any added later — which is the failure mode nine
   *  hand-written throws have. */
  const guardWrites = (): void => {
    if (host.writesRefusedReason) throw new Error(host.writesRefusedReason);
  };

  /** Fires the host's write observer, success or failure, from inside the
   *  body — a wrapper outside it could not see a partial `orderIds` on a
   *  thrown `closePosition`. */
  const observe = (
    method: string,
    account: string,
    summary: string,
    ok: boolean,
    extra: Omit<HostWriteEvent, 'method' | 'account' | 'summary' | 'ok'> = {},
  ): void => {
    host.onWrite?.({ method, account, summary, ok, ...extra });
  };

  const summarizeBalance = (accounts: readonly string[] | undefined): CombinedBalance => {
    // `accounts ?? streaming` and not `accounts?.length ? … : streaming` — an
    // explicitly empty list means "no accounts", not "all of them". The three
    // old copies disagreed on exactly this.
    const names = accounts ?? host.readStreamingAccounts();
    let totalEquity = 0;
    let totalAvailable = 0;
    let totalUsed = 0;
    let totalUpnl = 0;
    let longExposure = 0;
    let shortExposure = 0;
    for (const name of names) {
      const store = host.getStore(name);
      if (!store) continue;
      const balance = store.balance;
      if (balance) {
        totalEquity += balance.total ?? 0;
        totalAvailable += balance.free ?? 0;
        totalUsed += balance.used ?? 0;
        totalUpnl += balance.upnl ?? 0;
      }
      for (const position of store.positions ?? []) {
        if (position.side === 'long') longExposure += position.notional;
        else if (position.side === 'short') shortExposure += position.notional;
      }
    }
    return { totalEquity, totalAvailable, totalUsed, totalUpnl, longExposure, shortExposure, currency: 'USD' };
  };

  return {
    // ── Ambient ──────────────────────────────────────────────────────────
    getActiveAccount: () => host.readActiveAccount(),
    getActiveAccounts: () => host.readStreamingAccounts(),
    getActiveSymbol: () => host.readActiveSymbol(),
    getActiveExchange: (): ExchangeName => {
      const account = host.readActiveAccount();
      // The binding first: web resolves this through a name map with fallbacks
      // the body cannot see, and without it an unknown account reads as the
      // default rather than as an error.
      const named = host.readExchangeNameForAccount?.(account);
      if (named) return lower(named, 'bybit');
      const fromExchange = host.getExchange(account)?.name;
      if (fromExchange) return lower(fromExchange, 'bybit');
      const row = host.readAllAccounts().find((candidate) => candidate.name === account);
      return lower(row?.exchange, 'bybit');
    },
    getAccounts: (): readonly AccountInfo[] => {
      const streaming = new Set(host.readStreamingAccounts());
      return host.readAllAccounts().map((account) => ({
        name: account.name,
        exchange: lower(account.exchange, 'bybit'),
        isStreaming: streaming.has(account.name),
      }));
    },

    // ── Store reads ──────────────────────────────────────────────────────
    getMarkets: (account): readonly Market[] =>
      host.readMarkets?.(account) ?? ((host.getStore(account)?.markets as readonly Market[]) ?? EMPTY),
    getMarket: (symbol, account): Market | undefined => findMarket(symbol, account),
    getTicker: (symbol, account): Ticker | undefined => findTicker(symbol, account),
    getTickers: (symbols, account): readonly Ticker[] => {
      if (host.readTickers) return host.readTickers(symbols, account);
      const tickers = host.getStore(account)?.tickers;
      if (!tickers) return EMPTY;
      // An empty `symbols` list means NO symbols, not every symbol — the same
      // rule `getCombinedBalance` applies to an empty account list. Mobile's
      // copy returned all of them.
      const want = new Set(symbols);
      return tickers.filter((ticker) => want.has(ticker.symbol));
    },
    getPositions: ({ account, symbol }): readonly Position[] =>
      bySymbol((host.getStore(account)?.positions as readonly Position[]) ?? EMPTY, symbol),
    getOrders: ({ account, symbol }): readonly Order[] =>
      bySymbol((host.getStore(account)?.orders as readonly Order[]) ?? EMPTY, symbol),
    getBalance: (account): Balance | undefined =>
      host.readBalance ? host.readBalance(account) : host.getStore(account)?.balance,
    getCombinedBalance: ({ accounts }): CombinedBalance => summarizeBalance(accounts),

    // ── Reads with a host-owned source ───────────────────────────────────
    getHistoricalPosition: (opts): HistoricalPosition | undefined | Promise<HistoricalPosition | undefined> =>
      host.readHistoricalPosition ? host.readHistoricalPosition(opts) : undefined,
    getOrderbook: (symbol, opts) => host.readOrderbook(symbol, opts),
    getCandles: async (symbol, timeframe, opts): Promise<readonly Candle[]> => {
      if (host.readCandles) return host.readCandles(symbol, timeframe, opts);
      const exchange = requireExchange(opts.account);
      // Throws rather than returning empty: "this venue has no candles" and
      // "this venue returned no candles" are different answers, and the web
      // copy used to conflate them by returning `[]` for both. The wording is
      // the CLI's, which said which capability was missing.
      const fetchOHLCV = exchange.fetchOHLCV;
      if (!fetchOHLCV) {
        throw new Error(`getCandles: account '${opts.account}' does not support OHLCV candles`);
      }
      const candles = await fetchOHLCV({
        symbol,
        interval: timeframe,
        from: opts.from,
        to: opts.to,
        limit: opts.limit,
      });
      return candles ?? EMPTY;
    },

    // ── Writes ───────────────────────────────────────────────────────────
    placeOrder: async (opts, account) => {
      guardWrites();
      const exchange = requireExchange(account);
      const placeOrder = requireMethod(exchange, 'placeOrder', account);
      const summary = `placeOrder ${opts.symbol} ${opts.side} ${opts.amount}`;
      try {
        const order = toSafeCexPlaceOrder(opts, account);
        const ids = await placeOrder(clampReduceOnly(order, account, requireMarket(opts.symbol, account)));
        const orderIds = Object.freeze((ids ?? []).slice());
        observe('placeOrder', account, summary, true, { orderIds });
        return { orderIds };
      } catch (error) {
        observe('placeOrder', account, summary, false, { error: messageOf(error) });
        throw error;
      }
    },
    placeOrders: async (orders, account) => {
      guardWrites();
      const exchange = requireExchange(account);
      const placeOrders = requireMethod(exchange, 'placeOrders', account);
      const summary = `placeOrders ${orders.length}x ${orders[0]?.symbol ?? ''}`.trim();
      try {
        const cache = new Map<string, Market>();
        const payload = orders.map((order) =>
          clampReduceOnly(
            toSafeCexPlaceOrder(order, account, cache),
            account,
            requireMarket(order.symbol, account, cache),
          ),
        );
        const ids = await placeOrders(payload);
        const orderIds = Object.freeze((ids ?? []).slice());
        observe('placeOrders', account, summary, true, { orderIds });
        return { orderIds };
      } catch (error) {
        const partial = ScriptBatchOrderError.from(error);
        observe('placeOrders', account, summary, false, {
          error: messageOf(error),
          ...(partial && {
            partialOrderIds: partial.partialOrderIds,
            ...(partial.outcome !== undefined && { outcome: partial.outcome }),
            ...(partial.partialOrderCount !== undefined && { partialOrderCount: partial.partialOrderCount }),
            ...(partial.totalOrderCount !== undefined && { totalOrderCount: partial.totalOrderCount }),
          }),
        });
        throw partial ?? error;
      }
    },
    cancelOrder: async (orderId, { account, symbol }) => {
      guardWrites();
      const exchange = requireExchange(account);
      const cancelOrders = requireMethod(exchange, 'cancelOrders', account);
      const summary = `cancelOrder ${orderId}`;
      try {
        // Re-resolved against the host's own store: the caller's copy is a
        // snapshot, and several venues cancel-and-replace from the object they
        // are handed.
        const order = (host.getStore(account)?.orders ?? []).find(
          (candidate) => candidate.id === orderId && (symbol === undefined || candidate.symbol === symbol),
        );
        if (!order) throw new Error(`cancelOrder: no order with id '${orderId}' on account '${account}'`);
        await cancelOrders([order]);
        observe('cancelOrder', account, summary, true);
      } catch (error) {
        observe('cancelOrder', account, summary, false, { error: messageOf(error) });
        throw error;
      }
    },
    cancelOrders: async (filter: CancelFilter, account) => {
      guardWrites();
      const exchange = requireExchange(account);
      const cancelOrders = requireMethod(exchange, 'cancelOrders', account);
      let candidates = (host.getStore(account)?.orders ?? []).slice();
      // An id list answers the question the other fields exist to ask, so it
      // REPLACES them — a caller that enumerated 25 of 40 resting orders must
      // not have symbol/side widen that back to 40.
      if (filter.ids !== undefined) {
        const wanted = new Set(filter.ids);
        candidates = candidates.filter((order) => wanted.has(order.id));
      } else {
        if (filter.symbol !== undefined) candidates = candidates.filter((o) => o.symbol === filter.symbol);
        if (filter.side !== undefined) candidates = candidates.filter((o) => o.side === filter.side);
        if (filter.reduceOnly !== undefined) {
          const want = Boolean(filter.reduceOnly);
          candidates = candidates.filter((o) => Boolean(o.reduceOnly) === want);
        }
      }
      const summary = `cancelOrders ${candidates.length}x ${filter.symbol ?? ''}`.trim();
      if (candidates.length === 0) return { canceled: 0 };
      try {
        await cancelOrders(candidates);
        observe('cancelOrders', account, summary, true);
        return { canceled: candidates.length };
      } catch (error) {
        observe('cancelOrders', account, summary, false, { error: messageOf(error) });
        throw error;
      }
    },
    updateOrder: async (orderId, patch: UpdateOrderPatch, account) => {
      guardWrites();
      const exchange = requireExchange(account);
      const updateOrder = requireMethod(exchange, 'updateOrder', account);
      const summary = `updateOrder ${orderId}`;
      try {
        const order = (host.getStore(account)?.orders ?? []).find((candidate) => candidate.id === orderId);
        if (!order) throw new Error(`updateOrder: no order with id '${orderId}' on account '${account}'`);
        const market = requireMarket(order.symbol, account);
        // Amends round to NEAREST for both fields, never the placement rule:
        // truncating a resting order's size is a change nobody asked for.
        const update = patch.update;
        const rounded: { price?: number; amount?: number } = { ...update };
        // A key present and `undefined` is not the same as an absent key: the
        // venue-side `updateOrder` decides "price or amount?" with
        // `Object.hasOwn`, so an explicit `undefined` would read as sent.
        if ('price' in update && update.price === undefined) delete rounded.price;
        if ('amount' in update && update.amount === undefined) delete rounded.amount;
        if ('price' in update && update.price !== undefined) {
          const price = roundOrderPrice(update.price, market);
          if (!Number.isFinite(price) || price <= 0) {
            throw new ScriptValidationError(`updateOrder: invalid price for '${order.symbol}': ${update.price}`);
          }
          assertMinimumPrice(price, market);
          rounded.price = price;
        }
        if ('amount' in update && update.amount !== undefined) {
          const amount = roundOrderSizeToNearest(update.amount, market);
          if (!Number.isFinite(amount) || amount < 0) {
            throw new ScriptValidationError(`updateOrder: invalid amount for '${order.symbol}': ${update.amount}`);
          }
          rounded.amount = amount;
        }
        await updateOrder({ ...patch, update: rounded, order } as UpdateOrderOpts);
        observe('updateOrder', account, summary, true);
      } catch (error) {
        observe('updateOrder', account, summary, false, { error: messageOf(error) });
        throw error;
      }
    },
    closePosition: async (opts: Omit<ClosePositionOpts, 'account'>, account) => {
      guardWrites();
      // Fail fast on a disconnected account, but do NOT reuse this handle for
      // the writes: `closePosition` awaits once per leg, and a reconnect in
      // that window disposes the exchange and builds a new one under the same
      // name, so a hedge-mode second leg would be sent to a dead object.
      requireExchange(account);
      const summary = `closePosition ${opts.symbol}${opts.side ? ` ${opts.side}` : ''}`;
      const orderIds: string[] = [];
      try {
        const candidates = ((host.getStore(account)?.positions ?? []) as Position[]).filter(
          (position) => position.symbol === opts.symbol && (!opts.side || position.side === opts.side),
        );
        if (candidates.length === 0) return { orderIds: Object.freeze([] as readonly string[]) };
        const market = requireMarket(opts.symbol, account);
        for (const position of candidates) {
          // `position.contracts` is BASE and `placeOrder.amount` is NATIVE —
          // they differ by `contractSize`, so the old fallback asked a
          // thousandfold on OKX's `DOGE-USDT-SWAP` and a fraction of the
          // position on a Bybit inverse market. `opts.amount` is already
          // native by contract; only the fallback needed converting.
          const closeAmount = roundMarketCloseSize(opts.amount ?? nativePositionSize(position, market), market);
          if (!Number.isFinite(closeAmount) || closeAmount <= 0) {
            throw new ScriptValidationError(`closePosition: non-positive size for '${opts.symbol}'`);
          }
          const placeOrder = requireMethod(requireExchange(account), 'placeOrder', account);
          const ids = await placeOrder({
            symbol: opts.symbol,
            side: position.side === 'long' ? 'sell' : 'buy',
            type: 'market',
            amount: closeAmount,
            reduceOnly: true,
          } as PlaceOrderOpts);
          for (const id of ids ?? []) orderIds.push(id);
        }
        observe('closePosition', account, summary, true, { orderIds });
        return { orderIds: Object.freeze(orderIds) };
      } catch (error) {
        // Partial ids matter here: some legs may already be live.
        observe('closePosition', account, summary, false, { orderIds, error: messageOf(error) });
        throw error;
      }
    },
    setLeverage: async (symbol, leverage, account, side) => {
      guardWrites();
      const exchange = requireExchange(account);
      const setLeverage = requireMethod(exchange, 'setLeverage', account);
      const summary = `setLeverage ${symbol} ${leverage}${side ? ` ${side}` : ''}`;
      try {
        await setLeverage(symbol, leverage, side);
        observe('setLeverage', account, summary, true);
      } catch (error) {
        observe('setLeverage', account, summary, false, { error: messageOf(error) });
        throw error;
      }
    },
    setPositionMode: async (mode, account) => {
      guardWrites();
      const exchange = requireExchange(account);
      const change = requireMethod(exchange, 'changePositionMode', account);
      const summary = `setPositionMode ${String(mode)}`;
      try {
        await change(String(mode) === 'hedged');
        observe('setPositionMode', account, summary, true);
      } catch (error) {
        observe('setPositionMode', account, summary, false, { error: messageOf(error) });
        throw error;
      }
    },
    setMarginType: async (symbol, marginType, account) => {
      guardWrites();
      const exchange = requireExchange(account);
      const change = requireMethod(exchange, 'changeMarginType', account);
      const summary = `setMarginType ${symbol} ${String(marginType)}`;
      try {
        // `(isolated, symbol?)`, NOT `(symbol, marginType)`. Passing a symbol
        // where the boolean is read coerces truthy and sets isolated on every
        // call — which is what this did until the seam stopped using `unknown`.
        await change(String(marginType) === 'isolated', symbol);
        observe('setMarginType', account, summary, true);
      } catch (error) {
        observe('setMarginType', account, summary, false, { error: messageOf(error) });
        throw error;
      }
    },

    // ── Side channels ────────────────────────────────────────────────────
    /**
     * `guardWrites()` FIRST. `runCommand` is a write — it is in
     * `CORE_WRITE_API_PATHS`, and `OEMS_WRITE_API_PATHS` excludes it by name
     * as "the REPL escape hatch", because `runCommand('buy $1000')` places an
     * order without touching any other verb on this interface.
     *
     * It was the one write that did not call `guardWrites`, so a host
     * declaring `writesRefusedReason` still executed it. Nothing bound a
     * `runCommand` on such a host, so it was latent rather than live — but
     * "latent" is what the declared property exists to stop being load-bearing.
     * The refusal test enumerated nine methods by hand and missed this one,
     * which is precisely the by-hand omission the property replaced; it is
     * derived from the interface now.
     */
    runCommand: async (command, opts): Promise<RunCommandResult> => {
      guardWrites();
      if (!host.runCommand) {
        throw new Error('runCommand is not available on this host');
      }
      return host.runCommand(command, opts);
    },
    print: (text: string, kind: PrintKind) => host.print?.(text, kind),

    signer: undefined,
  };
};

const messageOf = (error: unknown): string =>
  error instanceof Error ? error.message : typeof error === 'string' ? error : 'Unknown error';
