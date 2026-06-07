import { TimeLimitedQueue } from '@tealstreet/data-structures/src/TimeLimitedQueue';
import { exchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';
import {
  ActiveChaser,
  ActiveTWAP,
  DisplayedPosition,
  Market,
  Order,
  OrderSide,
  OrderStatus,
  OrderType,
  PositionSide,
  StoreData,
  Ticker,
  Writable,
} from '../../types';
import { TwapStatus } from '../twap/TwapStatus';
import { BasePostprocessor } from './BasePostprocessor';

const positionKey = (symbol: string, side: PositionSide) => `${symbol}:${side}`;

type StopTpCoverage = { hasFullStop: boolean; hasFullTP: boolean; stopAmount: number; tpAmount: number };

/**
 * Adds ticker, market, balance, exchange information to positions to save some UI cycles.
 * Also calculates 5 min pnl.
 */
export class DisplayedPositionsPostprocessor extends BasePostprocessor {
  private calculateUnrealizedPnl(
    position: { unrealizedPnl: number; side: PositionSide; entryPrice: number; contracts: number },
    ticker: { last: number } | undefined,
    alwaysCalculateUnrealizedPnlFromLastPrice: boolean,
  ): number {
    if ((position.unrealizedPnl || !ticker || ticker.last === 0) && !alwaysCalculateUnrealizedPnlFromLastPrice) {
      return position.unrealizedPnl;
    }
    // If we're forcing calculation but ticker is still undefined, return existing unrealizedPnl
    if (!ticker || ticker.last === 0) {
      return position.unrealizedPnl;
    }
    return (
      (position.side === PositionSide.Long ? ticker.last - position.entryPrice : position.entryPrice - ticker.last) *
      Math.abs(position.contracts)
    );
  }

  private getChaserPositionSide(chaser: { side: OrderSide; reduceOnly?: boolean }): PositionSide {
    if (chaser.side === OrderSide.Buy) {
      return chaser.reduceOnly ? PositionSide.Short : PositionSide.Long;
    }
    return chaser.reduceOnly ? PositionSide.Long : PositionSide.Short;
  }

  private getTwapPositionSide(twap: { side: OrderSide }): PositionSide {
    return twap.side === OrderSide.Buy ? PositionSide.Long : PositionSide.Short;
  }

  private buildActiveChaserKeys(activeChasers: ActiveChaser[]): Record<string, true> {
    const keys: Record<string, true> = {};
    for (let i = 0; i < activeChasers.length; i++) {
      const chaser = activeChasers[i];
      if (chaser.isStopped) continue;
      keys[positionKey(chaser.symbol, this.getChaserPositionSide(chaser))] = true;
    }
    return keys;
  }

  private buildActiveTwapKeys(activeTWAPs: ActiveTWAP[]): Record<string, true> {
    const keys: Record<string, true> = {};
    for (let i = 0; i < activeTWAPs.length; i++) {
      const twap = activeTWAPs[i];
      const activeMarket = twap.activeMarket;
      if (twap.status !== TwapStatus.Running || !activeMarket) continue;
      keys[positionKey(activeMarket.symbol, this.getTwapPositionSide(twap))] = true;
    }
    return keys;
  }

  // Maps each `symbol:closingSide` to its open stop/take-profit coverage so the
  // displayed position can carry `hasFullStop` / `hasFullTP` precomputed instead
  // of the UI re-scanning every account's orders per render. A Sell order closes
  // a Long, a Buy order closes a Short; `amount === 0` is the exchange's
  // close-entire-position marker (full coverage regardless of size).
  private buildStopTpCoverage(orders: Order[]): Record<string, StopTpCoverage> {
    const coverage: Record<string, StopTpCoverage> = {};
    for (let i = 0; i < orders.length; i++) {
      const order = orders[i];
      if (order.status !== OrderStatus.Open) continue;

      const closingSide =
        order.side === OrderSide.Sell
          ? PositionSide.Long
          : order.side === OrderSide.Buy
            ? PositionSide.Short
            : undefined;
      if (!closingSide) continue;

      const isStop = order.type === OrderType.StopLoss || order.type === OrderType.TrailingStopLoss;
      const isTakeProfit = order.type === OrderType.TakeProfit || order.type === OrderType.Limit;
      if (!isStop && !isTakeProfit) continue;

      const key = positionKey(order.symbol, closingSide);
      let entry = coverage[key];
      if (!entry) {
        entry = { hasFullStop: false, hasFullTP: false, stopAmount: 0, tpAmount: 0 };
        coverage[key] = entry;
      }

      if (isStop) {
        if (order.amount === 0) entry.hasFullStop = true;
        else entry.stopAmount += order.amount || 0;
      }
      if (isTakeProfit) {
        if (order.amount === 0) entry.hasFullTP = true;
        else entry.tpAmount += order.amount || 0;
      }
    }
    return coverage;
  }

  private _lastMinutePnl = new Map<string, TimeLimitedQueue<{ upnl: number }>>();
  private _lastFiveMinutesPnl = new Map<string, TimeLimitedQueue<{ upnl: number }>>();
  // Cache the symbol-keyed lookup tables across postprocessor passes —
  // the source `tickers` / `markets` arrays are only re-replaced on full
  // snapshot updates (not on per-frame ticker mutations, which the store
  // applies in place). Reference-equal arrays mean the maps are still
  // correct. Saves ~600 map inserts per pass on Bybit linear at 4 Hz.
  private _cachedTickerMap: Record<string, Ticker> = {};
  private _cachedMarketMap: Record<string, Market> = {};
  private _cachedTickerSource: Ticker[] | undefined;
  private _cachedMarketSource: Market[] | undefined;
  private trackPositionPnl(position: Writable<DisplayedPosition>): void {
    if (!position.ticker?.last) return;

    const id = `${position.account}:${position.id}`;

    // Track 5-minute PNL
    let lastFiveMinutesPnl = this._lastFiveMinutesPnl.get(id);
    if (!lastFiveMinutesPnl) {
      lastFiveMinutesPnl = new TimeLimitedQueue<{ upnl: number }>(300);
      this._lastFiveMinutesPnl.set(id, lastFiveMinutesPnl);
    }
    lastFiveMinutesPnl.enqueue({ upnl: position.unrealizedPnl });
    position.fiveMinUpnl = lastFiveMinutesPnl.peek()?.upnl || 0;

    // Track 1-minute PNL
    let lastMinutePnl = this._lastMinutePnl.get(id);
    if (!lastMinutePnl) {
      lastMinutePnl = new TimeLimitedQueue<{ upnl: number }>(60);
      this._lastMinutePnl.set(id, lastMinutePnl);
    }
    lastMinutePnl.enqueue({ upnl: position.unrealizedPnl });
    position.oneMinUpnl = lastMinutePnl.peek()?.upnl || 0;
  }

  process(store: Writable<StoreData>): void {
    let alwaysCalculateUnrealizedPnlFromLastPrice =
      exchangeMetadata[this._exchangeName]?.alwaysCalculateUnrealizedPnlFromLastPrice || false;

    if (this._options.tuning?.calculateUpnlFromRealtimeLastPrice) {
      alwaysCalculateUnrealizedPnlFromLastPrice = true;
    }

    // Pre-map tickers and markets by symbol for O(1) lookups, with
    // reference-equality caching: the store mutates ticker/market
    // *fields* in place per WS frame but only swaps the array
    // *reference* on full snapshot replacement (initial fetch,
    // markets:changed, ticker-list rebuild). When the source array
    // reference is unchanged, the cached map is still valid because
    // its values are the same ticker objects (mutated in place by
    // the store). Saves ~600 map inserts per pass on Bybit linear.
    const tickers = store.tickers;
    let tickerMap = this._cachedTickerMap;
    if (this._cachedTickerSource !== tickers) {
      tickerMap = {};
      for (let i = 0; i < tickers.length; i++) {
        const t = tickers[i];
        tickerMap[t.symbol] = t;
      }
      this._cachedTickerMap = tickerMap;
      this._cachedTickerSource = tickers;
    }
    const markets = store.markets;
    let marketMap = this._cachedMarketMap;
    if (this._cachedMarketSource !== markets) {
      marketMap = {};
      for (let i = 0; i < markets.length; i++) {
        const m = markets[i];
        marketMap[m.symbol] = m;
      }
      this._cachedMarketMap = marketMap;
      this._cachedMarketSource = markets;
    }

    const hasActiveChasers = store.activeChasers.length > 0;
    const hasActiveTWAPs = store.activeTWAPs.length > 0;
    const activeChaserKeys = hasActiveChasers ? this.buildActiveChaserKeys(store.activeChasers) : undefined;
    const activeTwapKeys = hasActiveTWAPs ? this.buildActiveTwapKeys(store.activeTWAPs) : undefined;
    const realPositionKeys: Record<string, true> | undefined = hasActiveChasers ? {} : undefined;
    const needsPositionKey = hasActiveChasers || hasActiveTWAPs;
    const stopTpCoverage = this.buildStopTpCoverage(store.orders || []);
    let hasStopTpCoverage = false;
    for (const _key in stopTpCoverage) {
      hasStopTpCoverage = true;
      break;
    }
    const displayedPositions: Writable<DisplayedPosition[]> = [];
    const positions = store.positions || [];

    for (let i = 0; i < positions.length; i++) {
      const position = positions[i];
      const activeKey = needsPositionKey || hasStopTpCoverage ? positionKey(position.symbol, position.side) : undefined;
      if (realPositionKeys && activeKey) {
        realPositionKeys[activeKey] = true;
      }

      const contracts = Math.abs(position.contracts);
      if (contracts <= 0) continue;

      const ticker = tickerMap[position.symbol];
      const coverage = activeKey ? stopTpCoverage[activeKey] : undefined;
      const displayedPosition = {
        ...position,
        contracts,
        unrealizedPnl: this.calculateUnrealizedPnl(position, ticker, alwaysCalculateUnrealizedPnlFromLastPrice),
        key: `${this._accountName}:${position.symbol}:${position.side}`,
        account: this._accountName ? this._accountName : '',
        ticker,
        market: marketMap[position.symbol],
        balance: store.balance,
        exchangeName: this._exchangeName!,
        hasActiveChaser: activeKey !== undefined && activeChaserKeys?.[activeKey] === true,
        hasActiveTWAP: activeKey !== undefined && activeTwapKeys?.[activeKey] === true,
        hasFullStop: coverage ? coverage.hasFullStop || coverage.stopAmount >= contracts : false,
        hasFullTP: coverage ? coverage.hasFullTP || coverage.tpAmount >= contracts : false,
      } as Writable<DisplayedPosition> & { key: string };
      this.trackPositionPnl(displayedPosition);
      displayedPositions.push(displayedPosition);
    }

    if (realPositionKeys) {
      this.appendFakePositionsFromChasers(store, tickerMap, marketMap, realPositionKeys, displayedPositions);
    }

    store.displayedPositions = displayedPositions;
  }

  private appendFakePositionsFromChasers(
    store: StoreData,
    tickerMap: Record<string, Ticker>,
    marketMap: Record<string, Market>,
    realPositionKeys: Record<string, true>,
    displayedPositions: Writable<DisplayedPosition[]>,
  ): void {
    // Check for active chasers that haven't filled any orders yet (first buy-in)
    for (const chaser of store.activeChasers) {
      if (chaser.isStopped || chaser.reduceOnly) {
        continue;
      }

      // Only create fake position if no real position exists
      const fakePositionSide = chaser.side === OrderSide.Buy ? PositionSide.Long : PositionSide.Short;
      if (!realPositionKeys[positionKey(chaser.symbol, fakePositionSide)]) {
        const ticker = tickerMap[chaser.symbol];
        const market = marketMap[chaser.symbol];

        if (ticker && market) {
          const fakePosition: Writable<DisplayedPosition> = {
            // Required Position fields
            id: `fake-chaser-${chaser.id}`,
            symbol: chaser.symbol,
            side: fakePositionSide,
            entryPrice: ticker.last || 0,
            notional: 0,
            // Pre-existing latent typo: `maxLeverage` doesn't exist on Market;
            // accessing it returned `undefined` and the `|| 1` always won.
            // Cast to preserve historical behavior — fixing to
            // `market.limits.leverage.max` is a semantic change.
            leverage: (market as { maxLeverage?: number }).maxLeverage || 1,
            unrealizedPnl: 0,
            contracts: 0, // Show 0 contracts as it's pending
            liquidationPrice: 0,
            // Optional Position fields
            realizedPnl: 0,
            // Required DisplayedPosition fields
            account: this._accountName || '',
            ticker,
            market,
            balance: store.balance,
            exchangeName: this._exchangeName!,
            oneMinUpnl: 0,
            fiveMinUpnl: 0,
            hasActiveChaser: true,
            hasActiveTWAP: false,
            isFakePosition: true, // Mark as fake
            chaserSize: chaser.size, // Store intended size
            hasFullStop: false,
            hasFullTP: false,
          };

          this.trackPositionPnl(fakePosition);
          displayedPositions.push(fakePosition);
        }
      }
    }
  }
}
