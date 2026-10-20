import type { PackedTickerDelta, Store, StoreUpdate } from './store.interface';

import uniqBy from 'lodash/uniqBy';

import { Market, Order, OrderStatus, Position, RequestOutcome, StoreData, Ticker, Writable } from '../types';
import { clone } from '../utils/clone';
import { virtualClock } from '../utils/virtualClock';

// Longer than any REST snapshot is in flight; short enough that a wrong "final" heals.
const TERMINAL_ORDER_MEMORY_MS = 60_000;
const TERMINAL_ORDER_MEMORY_MAX = 1_000;
const TERMINAL_STATUSES = new Set<OrderStatus>([OrderStatus.Closed, OrderStatus.Canceled]);

export const PACKED_TICKER_FIELDS = [
  'bid',
  'ask',
  'last',
  'mark',
  'index',
  'percentage',
  'openInterest',
  'fundingRate',
  'fundingTime',
  'volume',
  'quoteVolume',
  'midPrice',
] as const;

type PackedTickerField = (typeof PACKED_TICKER_FIELDS)[number];

export const defaultStore: StoreData = {
  latency: 0,
  balance: { used: 0, free: 0, total: 0, upnl: 0 },
  markets: [],
  tickers: [],
  positions: [],
  displayedPositions: [],
  displayedMargin: { longExposure: 0, shortExposure: 0, totalNotional: 0 },
  ordersRisk: {},
  activeChasers: [],
  activeTWAPs: [],
  orders: [],
  messageHandlerStats: {},
  requestStats: { ok: 0, rateLimit: 0, auth: 0, network: 0, other: 0 },
  fetchedLeverage: {},
  loaded: {
    balance: false,
    orders: false,
    markets: false,
    tickers: false,
    positions: false,
  },
  options: {
    isHedged: false,
  },
  updateIds: {
    positions: 0,
    orders: 0,
  },
  socketHealth: {
    publicHealthy: false,
    privateHealthy: false,
    publicInitialized: false,
    privateInitialized: false,
  },
};

export class DefaultStore implements Store {
  private listeners = new Set<(data: StoreData, fromWorker?: boolean) => void>();
  private state: Writable<StoreData> = clone(defaultStore);
  private tickerIndexMap = new Map<string, number>();
  private marketIndexMap = new Map<string, number>();
  private marketsVersion = 0;
  private tickersVersion = 0;
  private pendingTickerDeltas = new Map<number, Partial<Ticker>>();

  // Hashmap versions for O(1) access (not included in serialization)
  private tickersMap: Record<string, Ticker> = {};
  private marketsMap: Record<string, Market> = {};
  private marketsBySymbolMap: Record<string, Market> = {};
  private tickersBySymbolMap: Record<string, Ticker> = {};

  constructor() {
    // Initialize index maps and hashmaps from default state
    this.state.tickers.forEach((ticker, idx) => {
      this.tickerIndexMap.set(ticker.id, idx);
      this.tickersMap[ticker.id] = ticker;
      this.tickersBySymbolMap[ticker.symbol] = ticker;
    });
    this.state.markets.forEach((market, idx) => {
      this.marketIndexMap.set(market.id, idx);
      this.marketsMap[market.id] = market;
      this.marketsBySymbolMap[market.symbol] = market;
    });
  }

  private buildIdIndexMap = <T extends { id: string }>(items: T[]): Record<string, number> => {
    const indexById = Object.create(null) as Record<string, number>;
    for (let i = 0; i < items.length; i++) {
      indexById[items[i].id] = i;
    }
    return indexById;
  };

  private buildIdLookup = (items: Array<{ id: string }>): Record<string, true> => {
    const ids = Object.create(null) as Record<string, true>;
    for (let i = 0; i < items.length; i++) {
      ids[items[i].id] = true;
    }
    return ids;
  };

  private findIndexById = <T extends { id: string }>(
    items: T[],
    id: string,
    indexById?: Record<string, number>,
  ): number => {
    return indexById ? (indexById[id] ?? -1) : items.findIndex((item) => item.id === id);
  };

  // constructor() {
  //   return new Proxy(this, {
  //     get(target, prop) {
  //       const origMethod = target[prop];
  //       if (typeof origMethod == 'function') {
  //         return function (...args) {
  //           console.log(`Called ${prop} with args:`, args);
  //           let result = origMethod.apply(target, args);
  //           return result;
  //         };
  //       } else {
  //         return origMethod;
  //       }
  //     },
  //   });
  // }

  serialize() {
    return {
      latency: this.latency,
      balance: { ...this.balance },
      markets: [...this.markets],
      tickers: [...this.tickers],
      positions: [...this.positions],
      displayedPositions: [...this.displayedPositions],
      displayedMargin: {
        longExposure: this.displayedMargin?.longExposure ?? 0,
        shortExposure: this.displayedMargin?.shortExposure ?? 0,
        totalNotional: this.displayedMargin?.totalNotional ?? 0,
      },
      ordersRisk: { ...this.ordersRisk },
      activeChasers: [...this.activeChasers],
      activeTWAPs: [...this.activeTWAPs],
      orders: [...this.orders],
      starkConfig: this.starkConfig ? { ...this.starkConfig } : undefined,
      messageHandlerStats: this.messageHandlerStats ? { ...this.messageHandlerStats } : undefined,
      // MUST be serialized. `stubStore` carries it, and `exchangeStoreCache`'s
      // `shapesCompatible` requires every stub key to be present — omitting it here
      // invalidates every warm-boot snapshot and silently cold-boots every account.
      requestStats: this.requestStats ? { ...this.requestStats } : undefined,
      fetchedLeverage: { ...this.fetchedLeverage },
      loaded: {
        balance: this.loaded.balance,
        orders: this.loaded.orders,
        markets: this.loaded.markets,
        tickers: this.loaded.tickers,
        positions: this.loaded.positions,
      },
      options: {
        isHedged: this.options.isHedged,
      },
      updateIds: {
        positions: this.updateIds.positions,
        orders: this.updateIds.orders,
      },
      socketHealth: this.socketHealth ? { ...this.socketHealth } : undefined,
    };
  }

  get messageHandlerStats() {
    return this.state.messageHandlerStats;
  }

  get latency() {
    return this.state.latency;
  }

  get balance() {
    return this.state.balance;
  }

  get markets() {
    return this.state.markets;
  }

  get tickers() {
    return this.state.tickers;
  }

  get orders() {
    return this.state.orders;
  }

  get ordersRisk() {
    return this.state.ordersRisk;
  }

  get positions() {
    return this.state.positions;
  }

  get displayedPositions() {
    return this.state.displayedPositions;
  }

  get displayedMargin() {
    return this.state.displayedMargin;
  }

  get activeChasers() {
    return this.state.activeChasers;
  }

  get activeTWAPs() {
    return this.state.activeTWAPs;
  }

  get loaded() {
    return this.state.loaded;
  }

  get updateIds() {
    return this.state.updateIds;
  }

  get options() {
    return this.state.options;
  }

  // Getter methods for hashmap access
  get tickersById() {
    return this.tickersMap;
  }

  get marketsById() {
    return this.marketsMap;
  }

  get tickersBySymbol() {
    return this.tickersBySymbolMap;
  }

  get marketsBySymbol() {
    return this.marketsBySymbolMap;
  }

  getMarketsVersion() {
    return this.marketsVersion;
  }

  getTickersVersion() {
    return this.tickersVersion;
  }

  static packTickerDeltas(deltas: Array<[number, Partial<Ticker>]>) {
    const packed: PackedTickerDelta[] = [];

    for (let i = 0; i < deltas.length; i++) {
      const [index, changes] = deltas[i];

      let mask = 0;
      const values: number[] = [];
      for (let fieldIndex = 0; fieldIndex < PACKED_TICKER_FIELDS.length; fieldIndex++) {
        const field = PACKED_TICKER_FIELDS[fieldIndex];
        const value = changes[field];
        if (typeof value === 'number') {
          mask |= 1 << fieldIndex;
          values.push(value);
        }
      }

      if (mask !== 0) {
        packed.push([index, mask, ...values]);
      }
    }

    return { packed };
  }

  consumeTickerDeltas() {
    if (this.pendingTickerDeltas.size === 0) {
      return [] as Array<[number, Partial<Ticker>]>;
    }
    const deltas = Array.from(this.pendingTickerDeltas.entries()).map(
      ([index, changes]) => [index, changes] as [number, Partial<Ticker>],
    );
    this.pendingTickerDeltas.clear();
    return deltas;
  }

  private stripTickerInfo = (ticker: Ticker): Ticker => {
    if (!ticker.info) return ticker;
    const { info: _info, ...tickerWithoutInfo } = ticker;
    return tickerWithoutInfo as Ticker;
  };

  private stripTickerInfoFromChanges = (changes: Partial<Ticker>): Partial<Ticker> => {
    if (!('info' in changes)) return changes;
    const { info: _info, ...changesWithoutInfo } = changes;
    return changesWithoutInfo;
  };

  private patchObjectInPlace = <T extends object>(target: T, source: T, options?: { stripInfo?: boolean }) => {
    let changed = false;
    const targetRecord = target as Record<string, unknown>;
    const sourceRecord = source as Record<string, unknown>;
    for (const key in source) {
      if (options?.stripInfo && key === 'info') continue;
      const nextValue = sourceRecord[key];
      if (targetRecord[key] !== nextValue) {
        targetRecord[key] = nextValue;
        changed = true;
      }
    }

    return changed;
  };

  private patchTickerInPlace = (target: Ticker, source: Ticker) => {
    let changed = false;
    const mutableTarget = target as Writable<Ticker>;

    if (target.bid !== source.bid) {
      mutableTarget.bid = source.bid;
      changed = true;
    }
    if (target.ask !== source.ask) {
      mutableTarget.ask = source.ask;
      changed = true;
    }
    if (target.last !== source.last) {
      mutableTarget.last = source.last;
      changed = true;
    }
    if (target.mark !== source.mark) {
      mutableTarget.mark = source.mark;
      changed = true;
    }
    if (target.index !== source.index) {
      mutableTarget.index = source.index;
      changed = true;
    }
    if (target.midPrice !== source.midPrice) {
      if (source.midPrice === undefined) delete mutableTarget.midPrice;
      else mutableTarget.midPrice = source.midPrice;
      changed = true;
    }
    if (target.percentage !== source.percentage) {
      mutableTarget.percentage = source.percentage;
      changed = true;
    }
    if (target.openInterest !== source.openInterest) {
      mutableTarget.openInterest = source.openInterest;
      changed = true;
    }
    if (target.fundingRate !== source.fundingRate) {
      mutableTarget.fundingRate = source.fundingRate;
      changed = true;
    }
    if (target.fundingTime !== source.fundingTime) {
      mutableTarget.fundingTime = source.fundingTime;
      changed = true;
    }
    if (target.volume !== source.volume) {
      mutableTarget.volume = source.volume;
      changed = true;
    }
    if (target.quoteVolume !== source.quoteVolume) {
      mutableTarget.quoteVolume = source.quoteVolume;
      changed = true;
    }

    return changed;
  };

  private patchWorkerTickersInPlace = (tickers: Ticker[]) => {
    if (tickers.length === 0 || tickers.length !== this.state.tickers.length) return null;

    for (let i = 0; i < tickers.length; i++) {
      const current = this.state.tickers[i];
      const incoming = tickers[i];
      if (current.id !== incoming.id || current.symbol !== incoming.symbol) return null;
    }

    let changed = false;
    for (let i = 0; i < tickers.length; i++) {
      changed = this.patchTickerInPlace(this.state.tickers[i], tickers[i]) || changed;
    }

    return changed;
  };

  private patchWorkerMarketsInPlace = (markets: Market[]) => {
    if (markets.length === 0 || markets.length !== this.state.markets.length) return null;

    for (let i = 0; i < markets.length; i++) {
      const current = this.state.markets[i];
      const incoming = markets[i];
      if (current.id !== incoming.id || current.symbol !== incoming.symbol) return null;
    }

    let changed = false;
    for (let i = 0; i < markets.length; i++) {
      changed = this.patchObjectInPlace(this.state.markets[i], markets[i]) || changed;
    }

    return changed;
  };

  get zkLinkConfig() {
    return this.state.zkLinkConfig;
  }

  get starkConfig() {
    return this.state.starkConfig;
  }

  get userData() {
    return this.state.userData;
  }

  get fetchedLeverage() {
    return this.state.fetchedLeverage;
  }

  get socketHealth() {
    return this.state.socketHealth;
  }

  updateSocketHealth(data: Partial<NonNullable<StoreData['socketHealth']>>) {
    this.state.socketHealth = {
      ...this.state.socketHealth!,
      ...data,
    };
    this.notify();
  }

  get requestStats() {
    return this.state.requestStats;
  }

  /**
   * One increment, in place, no notify.
   *
   * This runs on EVERY REST request across every account, so the two things it
   * must not do are allocate and notify. `update()` would run the whole
   * ticker/market/order merge pipeline and then `notify()`, manufacturing a
   * store delta per request — the exact main-thread churn that made request
   * logging too expensive to keep in the first place.
   *
   * Getting the value out is free instead: the throttled store ship already
   * serializes the whole state, so the counters ride along on a cadence that
   * exists whether or not anyone is counting.
   */
  recordRequestOutcome(outcome: RequestOutcome) {
    const stats = this.state.requestStats;
    if (!stats) return;
    stats[outcome] += 1;
  }

  auditPositions = (removeDupes = false) => {
    // check for dupes
    const positions = uniqBy(this.state.positions, 'id');
    if (positions.length !== this.state.positions.length) {
      console.warn('Duplicate positions detected...');
      if (removeDupes) {
        console.error('Removing duplicate positions');
        Object.assign(this.state, { positions });
      }
    }

    // check for negative contracts
    const negativePositions = positions.filter((p) => p.contracts < 0);
    if (negativePositions.length > 0) {
      console.warn(
        'Negative contracts detected:',
        negativePositions.map((p) => ({ id: p.id, symbol: p.symbol, side: p.side, contracts: p.contracts })),
      );
    }
  };

  // Ids a venue reported FINAL (markOrdersTerminal), so a REST snapshot that read
  // the venue before that event cannot reopen them. Never inferred from a removal
  // or a mapped status: several venues remove live orders, or map "Triggered" to Closed.
  private terminalOrderIds = new Map<string, number>();

  private isRememberedTerminal(id: string, now: number) {
    const markedAt = this.terminalOrderIds.get(id);
    if (markedAt === undefined) return false;
    if (now - markedAt < TERMINAL_ORDER_MEMORY_MS) return true;
    this.terminalOrderIds.delete(id);
    return false;
  }

  private keepTerminalOrdersClosed(incoming: Order[]): Order[] {
    if (this.terminalOrderIds.size === 0) return incoming;
    const now = virtualClock.getCurrentTimeMs();
    let kept: Order[] | undefined;
    incoming.forEach((order, index) => {
      const reopens = !TERMINAL_STATUSES.has(order.status) && this.isRememberedTerminal(order.id, now);
      if (reopens && !kept) kept = incoming.slice(0, index);
      if (!kept) return;
      if (!reopens) {
        kept.push(order);
        return;
      }
      const current = this.state.orders.find((candidate) => candidate.id === order.id);
      if (current && TERMINAL_STATUSES.has(current.status)) kept.push(current);
    });
    return kept ?? incoming;
  }

  markOrdersTerminal = (orders: Array<Pick<Order, 'id'>>, status: OrderStatus.Closed | OrderStatus.Canceled) => {
    const now = virtualClock.getCurrentTimeMs();
    orders.forEach(({ id }) => {
      this.terminalOrderIds.delete(id);
      this.terminalOrderIds.set(id, now);
    });
    // Insertion order is age order; drop the oldest past the cap.
    for (const id of this.terminalOrderIds.keys()) {
      if (this.terminalOrderIds.size <= TERMINAL_ORDER_MEMORY_MAX) break;
      this.terminalOrderIds.delete(id);
    }
    this.updateOrdersStatus(orders, status);
  };

  update = (changes: StoreUpdate, fromWorker = false, source?: 'account-read') => {
    let normalizedChanges: Partial<Writable<StoreUpdate>> = changes;
    const ensureMutableChanges = () => {
      if (normalizedChanges === changes) {
        normalizedChanges = { ...changes };
      }
      return normalizedChanges;
    };
    let patchedWorkerTickers = false;
    let patchedWorkerMarkets = false;
    const packedTickerDeltas = changes.tickerDeltas;

    if (packedTickerDeltas !== undefined) {
      delete ensureMutableChanges().tickers;
      delete ensureMutableChanges().tickerDeltas;
    }

    if (changes.tickers && packedTickerDeltas === undefined) {
      const patchChanged = fromWorker ? this.patchWorkerTickersInPlace(changes.tickers) : null;
      if (patchChanged !== null) {
        delete ensureMutableChanges().tickers;
        patchedWorkerTickers = patchChanged;
      } else {
        normalizedChanges = { ...changes, tickers: changes.tickers.map(this.stripTickerInfo) };
      }
    }

    if (fromWorker && normalizedChanges.markets) {
      const patchChanged = this.patchWorkerMarketsInPlace(normalizedChanges.markets);
      if (patchChanged !== null) {
        delete ensureMutableChanges().markets;
        patchedWorkerMarkets = patchChanged;
      }
    }

    const rawIncomingUpdateIds = changes.updateIds;
    const hasWorkerOrderUpdateId = fromWorker && rawIncomingUpdateIds?.orders !== undefined;
    const hasWorkerPositionUpdateId = fromWorker && rawIncomingUpdateIds?.positions !== undefined;

    if (rawIncomingUpdateIds) {
      ensureMutableChanges().updateIds = {
        ...this.state.updateIds,
        ...rawIncomingUpdateIds,
      };
    }

    const incomingUpdateIds = normalizedChanges.updateIds;
    const workerOrdersUpdateId = incomingUpdateIds?.orders;
    const workerPositionsUpdateId = incomingUpdateIds?.positions;
    const shouldAdoptWorkerOrderUpdateId = hasWorkerOrderUpdateId;
    const shouldAdoptWorkerPositionUpdateId = hasWorkerPositionUpdateId;

    if (shouldAdoptWorkerOrderUpdateId && workerOrdersUpdateId === this.state.updateIds.orders) {
      delete ensureMutableChanges().orders;
    }
    if (shouldAdoptWorkerPositionUpdateId && workerPositionsUpdateId === this.state.updateIds.positions) {
      delete ensureMutableChanges().positions;
    }
    if (
      fromWorker &&
      incomingUpdateIds &&
      (workerOrdersUpdateId ?? this.state.updateIds.orders) === this.state.updateIds.orders &&
      (workerPositionsUpdateId ?? this.state.updateIds.positions) === this.state.updateIds.positions
    ) {
      delete ensureMutableChanges().updateIds;
    }

    const patchedWorkerTickerDeltas = packedTickerDeltas
      ? this.applyPackedTickerDeltasInPlace(packedTickerDeltas)
      : false;

    let hasChanges = patchedWorkerTickers || patchedWorkerMarkets || patchedWorkerTickerDeltas;
    for (const _key in normalizedChanges) {
      hasChanges = true;
      break;
    }
    if (!hasChanges) return;

    // A worker's store already kept its terminal orders closed.
    if (!fromWorker && normalizedChanges.orders) {
      const orders = this.keepTerminalOrdersClosed(normalizedChanges.orders as Order[]);
      if (orders !== normalizedChanges.orders) ensureMutableChanges().orders = orders;
    }
    Object.assign(this.state, normalizedChanges);
    if (normalizedChanges.positions) this.auditPositions(true);

    // Bump update counters when orders or positions are replaced wholesale.
    // The granular store methods (`addOrUpdateOrders`, `updateOrder`, etc.)
    // already do this; bulk replacement via `store.update({ orders })`
    // (Bybit `fetchAndSetPrivateSnapshot`, blofin / cryptocom / grvt
    // refreshes, in-process-public-data fan-out, etc.) was previously
    // skipping the bump, which left consumers like `ChaserOrderManager`
    // — gating their per-frame `data.orders.filter(...)` work on
    // `data.updateIds.orders` — blind to wholesale changes.
    if (normalizedChanges.orders !== undefined && !shouldAdoptWorkerOrderUpdateId && source !== 'account-read') {
      this.state.updateIds.orders += 1;
    }
    if (normalizedChanges.positions !== undefined && !shouldAdoptWorkerPositionUpdateId && source !== 'account-read') {
      this.state.updateIds.positions += 1;
    }

    // Rebuild index maps and hashmaps when tickers or markets are fully replaced
    if (normalizedChanges.tickers) {
      this.tickersVersion += 1;
      this.pendingTickerDeltas.clear();
      this.tickerIndexMap.clear();
      this.tickersMap = {};
      this.tickersBySymbolMap = {};
      normalizedChanges.tickers.forEach((ticker, idx) => {
        this.tickerIndexMap.set(ticker.id, idx);
        this.tickersMap[ticker.id] = ticker;
        this.tickersBySymbolMap[ticker.symbol] = ticker;
      });
    }
    if (normalizedChanges.markets) {
      this.marketsVersion += 1;
      this.marketIndexMap.clear();
      this.marketsMap = {};
      this.marketsBySymbolMap = {};
      normalizedChanges.markets.forEach((market, idx) => {
        this.marketIndexMap.set(market.id, idx);
        this.marketsMap[market.id] = market;
        this.marketsBySymbolMap[market.symbol] = market;
      });
    }
    this.notify(fromWorker);
  };

  reset = () => {
    this.state = clone(defaultStore);
    this.terminalOrderIds.clear();
    this.tickerIndexMap.clear();
    this.marketIndexMap.clear();
    this.marketsVersion = 0;
    this.tickersVersion = 0;
    this.pendingTickerDeltas.clear();
    this.tickersMap = {};
    this.marketsMap = {};
    this.marketsBySymbolMap = {};
    this.tickersBySymbolMap = {};
    this.notify();
  };

  subscribe = (cb: (data: StoreData) => void) => {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  };

  removeOrders = (orders: Array<Pick<Order, 'id'>>) => {
    if (orders.length === 0 || this.state.orders.length === 0) return;

    const idsToRemove = this.buildIdLookup(orders);
    const prevLength = this.state.orders.length;
    this.state.orders = this.state.orders.filter((order) => idsToRemove[order.id] !== true);
    if (this.state.orders.length === prevLength) return;

    this.state.updateIds.orders += 1;
    this.notify();
  };

  removeOrder = (order: Pick<Order, 'id'>) => {
    const idx = this.state.orders.findIndex((o) => o.id === order.id);
    if (idx > -1) {
      this.state.orders.splice(idx, 1);
      this.state.updateIds.orders += 1;
      this.notify();
    }
  };

  updateOrdersStatus = (orders: Array<Pick<Order, 'id'>>, newStatus: OrderStatus) => {
    if (orders.length === 0 || this.state.orders.length === 0) return;

    const idsToUpdate = this.buildIdLookup(orders);
    let didUpdate = false;
    for (let i = 0; i < this.state.orders.length; i++) {
      if (idsToUpdate[this.state.orders[i].id] === true) {
        this.updateInArray('orders', i, { status: newStatus });
        didUpdate = true;
      }
    }

    if (!didUpdate) return;
    this.state.updateIds.orders += 1;
    this.notify();
  };

  updateOrderStatus = (order: Pick<Order, 'id'>, newStatus: OrderStatus) => {
    this.updateOrder(order, { status: newStatus });
  };

  updateOrder = (order: Pick<Order, 'id'>, changes: Partial<Order>) => {
    const idx = this.state.orders.findIndex((o) => o.id === order.id);

    if (idx > -1) {
      this.updateInArray('orders', idx, changes);
      this.state.updateIds.orders += 1;
      this.notify();
    }
  };

  addOrder = (order: Order) => {
    const [kept] = this.keepTerminalOrdersClosed([order]);
    if (kept !== order) return;
    this.state.orders.push(order);
    this.state.updateIds.orders += 1;
    this.notify();
  };

  addOrUpdateOrder = (order: Order) => {
    const [kept] = this.keepTerminalOrdersClosed([order]);
    if (kept !== order) return;
    const idx = this.state.orders.findIndex((o) => o.id === order.id);
    if (idx > -1) {
      this.updateInArray('orders', idx, order);
    } else {
      this.state.orders.push(order);
    }
    this.state.updateIds.orders += 1;
    this.notify();
  };

  addOrUpdateOrders = (incoming: Order[]) => {
    const orders = this.keepTerminalOrdersClosed(incoming);
    if (orders.length === 0) return;

    const orderIndexById =
      orders.length > 1 && this.state.orders.length > 1 ? this.buildIdIndexMap(this.state.orders) : undefined;
    orders.forEach((order) => {
      const idx = this.findIndexById(this.state.orders, order.id, orderIndexById);
      if (idx > -1) {
        this.updateInArray('orders', idx, order);
      } else {
        if (orderIndexById) {
          orderIndexById[order.id] = this.state.orders.length;
        }
        this.state.orders.push(order);
      }
    });
    this.state.updateIds.orders += 1;
    this.notify();
  };

  updatePosition = (position: Pick<Position, 'id'>, changes: Partial<Position>) => {
    const idx = this.state.positions.findIndex((p) => p.id === position.id);
    if (idx > -1) {
      this.updateInArray('positions', idx, changes);
      this.state.updateIds.positions += 1;
      this.notify();
    }
  };

  addOrUpdatePosition = (position: Position) => {
    const idx = this.state.positions.findIndex((p) => p.id === position.id);

    if (idx > -1) {
      this.updateInArray('positions', idx, position);
    } else {
      this.state.positions.push({ ...position });
    }
    this.auditPositions();

    this.state.updateIds.positions += 1;
    this.notify();
  };

  updatePositions = (updates: Array<[Pick<Position, 'id'>, Partial<Position>]>) => {
    if (updates.length === 0 || this.state.positions.length === 0) return;

    const positionIndexById =
      updates.length > 1 && this.state.positions.length > 1 ? this.buildIdIndexMap(this.state.positions) : undefined;
    const idexesChanges = updates.map(([position, changes]) => {
      const idx = this.findIndexById(this.state.positions, position.id, positionIndexById);
      return [idx, changes] as const;
    });

    if (idexesChanges.some(([idx]) => idx !== -1)) {
      idexesChanges.forEach(([idx, changes]) => this.updateInArray('positions', idx, changes));

      this.state.updateIds.positions += 1;
      this.notify();
    }
  };

  addOrUpdatePositions = (positions: Position[]) => {
    if (positions.length === 0) return;

    const positionIndexById =
      positions.length > 1 && this.state.positions.length > 1 ? this.buildIdIndexMap(this.state.positions) : undefined;
    positions.forEach((position) => {
      const idx = this.findIndexById(this.state.positions, position.id, positionIndexById);
      if (idx > -1) {
        this.updateInArray('positions', idx, position);
      } else {
        if (positionIndexById) {
          positionIndexById[position.id] = this.state.positions.length;
        }
        this.state.positions.push({ ...position });
      }
    });

    // Hoist outside the loop. `auditPositions` does an O(N) `uniqBy` +
    // O(N) negative-contracts filter; running it after every push/update
    // multiplied the cost by the batch size for no extra signal.
    this.auditPositions();

    this.state.updateIds.positions += 1;
    this.notify();
  };

  /**
   * Creates an empty position from actual position.
   *
   * Use to mark position is closed when wiping any other position information is not desired.
   *
   * @param position
   */
  createEmptyPositionFrom = (position: Position) => {
    return {
      ...position,
      entryPrice: 0,
      notional: 0,
      unrealizedPnl: 0,
      contracts: 0,
      liquidationPrice: 0,
      marginSize: 0,
      info: undefined,
      extra: undefined,
    };
  };

  /**
   * does the same thing as a function above but cleans up unannounced positions.
   *
   * @param positions
   */
  addOrUpdatePositionsFull = (positions: Position[]) => {
    const usedIds = new Set<string>();
    const positionIndexById =
      positions.length > 1 && this.state.positions.length > 1 ? this.buildIdIndexMap(this.state.positions) : undefined;
    positions.forEach((position) => {
      usedIds.add(position.id);
      const idx = this.findIndexById(this.state.positions, position.id, positionIndexById);
      if (idx > -1) {
        this.updateInArray('positions', idx, position);
      } else {
        if (positionIndexById) {
          positionIndexById[position.id] = this.state.positions.length;
        }
        this.state.positions.push({ ...position });
      }
    });
    Object.assign(this.state, {
      positions: this.state.positions.map((position) =>
        usedIds.has(position.id) ? position : this.createEmptyPositionFrom(position),
      ),
    });

    this.state.updateIds.positions += 1;
    this.notify();
  };

  /**
   * replaces order stash in a current exchange store completely
   *
   * @param orders
   */
  addOrUpdateOrdersFull = (orders: Order[]) => {
    Object.assign(this.state, {
      orders: this.keepTerminalOrdersClosed(orders),
    });
    this.state.updateIds.orders += 1;
    this.notify();
  };

  /**
   * replaces all orders for a given symbol with provided
   * @param symbol
   * @param orders
   */
  setOrdersForSymbol = (symbol: string, orders: Order[]) => {
    // Before the old rows go, so a terminal one can stand in for its reopened copy.
    const kept = this.keepTerminalOrdersClosed(orders);
    // remove old orders
    this.state.orders = this.state.orders.filter((o) => o.symbol !== symbol);
    // add new orders
    this.state.orders.push(...kept);

    this.state.updateIds.orders += 1;
    this.notify();
  };

  updateTicker = (ticker: Pick<Ticker, 'id'>, changes: Partial<Ticker>) => {
    // Resolve the stored ticker object directly via the `tickersMap`
    // hashmap. Skips the prior `tickerIndexMap.get + state.tickers[idx]`
    // double indirection — both routes return the same reference, but
    // the hashmap lookup is one fewer call frame per ticker frame
    // (~600/sec on Bybit linear).
    const item = this.tickersMap[ticker.id];
    if (!item) return;

    const normalizedChanges = this.stripTickerInfoFromChanges(changes);

    // Hot path: inline the field write + empty-check in a single
    // for-in pass instead of (a) `Object.keys(...).length === 0`
    // empty-check then (b) calling `updateInArray` which does its own
    // for-in. Saves one function-call frame and one extra for-in pass
    // per ticker frame.
    //
    // `pendingTickerDeltas` is deliberately NOT populated here. Delta
    // transport is produced directly by ExchangeWorker from its last-sent
    // ticker baseline; this legacy queue remains only for the public
    // consume/apply compatibility surface and deterministic store tests.
    let didChange = false;
    for (const k in normalizedChanges) {
      didChange = true;
      (item as any)[k] = (normalizedChanges as any)[k];
    }
    if (!didChange) return;

    // Hashmap refresh — references unchanged, but defensive in case
    // a future path swaps the underlying ticker object.
    this.tickersMap[item.id] = item;
    this.tickersBySymbolMap[item.symbol] = item;

    this.notify();
  };

  applyTickerDeltas = (deltas: Array<[number, Partial<Ticker>]>, fromWorker = false) => {
    if (!deltas.length) return;

    let didMutate = false;
    for (let i = 0; i < deltas.length; i++) {
      const [idx, rawChanges] = deltas[i];
      if (idx <= -1 || idx >= this.state.tickers.length) continue;

      const changes = this.stripTickerInfoFromChanges(rawChanges);
      // Empty-check via `for...in` instead of `Object.keys(...).length === 0`
      // — avoids allocating a keys array per applied delta.
      let hasAny = false;
      for (const _k in changes) {
        hasAny = true;
        break;
      }
      if (!hasAny) continue;

      this.updateInArray('tickers', idx, changes);
      didMutate = true;
    }

    if (didMutate) {
      this.notify(fromWorker);
    }
  };

  private applyPackedTickerDeltasInPlace = (deltas: PackedTickerDelta[]) => {
    if (!deltas.length) return false;

    let didMutate = false;
    for (let i = 0; i < deltas.length; i++) {
      const delta = deltas[i];
      const idx = delta[0];
      const mask = delta[1];

      if (idx < 0 || idx >= this.state.tickers.length || mask === 0) {
        continue;
      }

      const ticker = this.state.tickers[idx];
      let valueCursor = 2;

      for (let fieldIndex = 0; fieldIndex < PACKED_TICKER_FIELDS.length; fieldIndex++) {
        if (mask & (1 << fieldIndex)) {
          const field = PACKED_TICKER_FIELDS[fieldIndex] as PackedTickerField;
          const value = delta[valueCursor++];
          if (field === 'midPrice' && Number.isNaN(value)) delete (ticker as any)[field];
          else (ticker as any)[field] = value;
          didMutate = true;
        }
      }
    }

    return didMutate;
  };

  applyPackedTickerDeltas = (deltas: PackedTickerDelta[], fromWorker = false) => {
    if (this.applyPackedTickerDeltasInPlace(deltas)) {
      this.notify(fromWorker);
    }
  };

  updateStarkConfig(data: StoreData['starkConfig']) {
    this.state.starkConfig = data;
    this.notify();
  }

  updateZkLinkConfig(data: StoreData['zkLinkConfig']) {
    this.state.zkLinkConfig = data;
    this.notify();
  }

  updateUserData(data: StoreData['userData']) {
    this.state.userData = data;
    this.notify();
  }

  updateMarket = (market: Pick<Market, 'id'>, changes: Partial<Market>) => {
    const idx = this.marketIndexMap.get(market.id);

    if (idx !== undefined) {
      this.updateInArray('markets', idx, changes);
      this.marketsVersion += 1;
      this.notify();
    }
  };

  setSetting = (key: keyof StoreData['options'], value: boolean) => {
    if (this.state.options[key] !== value) {
      this.state.options[key] = value;
      this.notify();
    }
  };

  private updateInArray = <K extends 'markets' | 'orders' | 'positions' | 'tickers'>(
    key: K,
    idx: number,
    changes: Partial<StoreData[K][number]>,
  ) => {
    if (idx > -1 && key in this.state) {
      // Mutate properties directly instead of creating a new object via spread.
      // This reduces GC pressure from frequent updates (tickers, orders, positions)
      // notify() is still called to signal changes to subscribers.
      // Hot path: `for...in` over `Object.keys(changes)` saves an N-element
      // array allocation per call. Callers always pass plain object literals,
      // so no inherited enumerable concerns.
      const item = this.state[key][idx];
      for (const changeKey in changes) {
        (item as any)[changeKey] = (changes as any)[changeKey];
      }

      // Update hashmaps for tickers and markets (references stay the same)
      if (key === 'tickers') {
        const ticker = this.state.tickers[idx];
        this.tickersMap[ticker.id] = ticker;
        this.tickersBySymbolMap[ticker.symbol] = ticker;
      } else if (key === 'markets') {
        const market = this.state.markets[idx];
        this.marketsMap[market.id] = market;
        this.marketsBySymbolMap[market.symbol] = market;
      }
    }
  };

  private notify = (fromWorker = false) => {
    if (this.listeners.size > 0) {
      this.listeners.forEach((cb) => cb(this.state, fromWorker));
    }
  };
}
