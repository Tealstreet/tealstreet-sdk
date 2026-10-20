import type { Store } from '../../../store/store.interface';
import type { Balance, FetchedLeverage, Market, Order, OrderFillEvent, Position } from '../../../types';
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../../types';

// TxFlow account reads are unsigned. The subject is applicationId (the main
// wallet), never the locally held approved agent's address.
type Context = {
  store: Store;
  emitFill: (fill: OrderFillEvent) => void;
  reportError: (error: Error) => void;
  requestInfo: <T>(body: Record<string, unknown>) => Promise<T>;
  wallet: () => string;
  marketById: (id: string) => Market | undefined;
  disposed: () => boolean;
  onOrders?: () => void;
};
type Numeric = string | number;
type Leverage = {
  type: string;
  mode?: string;
  value?: Numeric;
  long_value?: Numeric;
  short_value?: Numeric;
};
type NativePosition = {
  type: string;
  position: {
    asset: string;
    szi: Numeric;
    leverage: { type: string; value: Numeric };
    positionValue: Numeric;
    unrealizedPnl: Numeric;
    marginUsed: Numeric;
    entryPx?: Numeric | null;
    liquidationPx?: Numeric | null;
    updateTime?: Numeric | null;
  };
};
type NativeState = {
  user?: string;
  time?: Numeric;
  marginSummary: { accountValue: Numeric; totalMarginUsed: Numeric; totalRawUsd: Numeric };
  assetPositions: NativePosition[];
};
type NativeOrder = {
  asset: number;
  oid: number;
  side: string;
  orderType: string;
  limitPx: Numeric;
  origSz: Numeric;
  sz: Numeric;
  timestamp: Numeric;
  isTrigger?: boolean;
  reduceOnly?: boolean;
  triggerPx?: Numeric | null;
  triggerType?: string;
  cloid?: string | null;
  state?: string;
  tif?: string;
  children?: unknown[] | null;
};
type NativeFill = {
  tid: number;
  oid: number;
  instrumentId: number;
  px: Numeric;
  sz: Numeric;
  side: string;
  time: Numeric;
};
type AssetData = { user?: string; coin: string; leverage: Leverage };
const PAGE = 500;
const FILL_CAP = 100_000;
function num(value: Numeric | undefined | null): number {
  if (value === null || value === undefined || value === '') throw new Error('Missing TxFlow account number');
  const n = Number(value);
  if (!Number.isFinite(n)) throw new Error('Invalid TxFlow account number');
  return n;
}
function id(value: number): string {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error('Invalid TxFlow native ID');
  return String(value);
}
function mode(type: string): MarginType {
  if (type.toLowerCase() === 'cross') return MarginType.Cross;
  if (type.toLowerCase() === 'isolated') return MarginType.Isolated;
  throw new Error('Unknown TxFlow margin mode');
}
function setting(leverage: Leverage): FetchedLeverage {
  if (leverage.mode !== undefined && !['oneWay', 'hedge'].includes(leverage.mode)) throw new Error('Unknown TxFlow position mode');
  const marginType = mode(leverage.type);
  const buyLeverage = num(leverage.mode === 'hedge' ? leverage.long_value : leverage.value);
  const sellLeverage = num(leverage.mode === 'hedge' ? leverage.short_value : leverage.value);
  if (buyLeverage <= 0 || sellLeverage <= 0) throw new Error('Invalid TxFlow leverage');
  return {
    buyLeverage,
    sellLeverage,
    marginType,
    ...(leverage.mode === undefined ? {} : { positionMode: leverage.mode === 'hedge' ? PositionMode.Hedged : PositionMode.OneWay }),
  };
}
function status(value: string): OrderStatus {
  if (value === 'Open' || value === 'PartialFilled') return OrderStatus.Open;
  if (value === 'Pending') return OrderStatus.Creating;
  if (value === 'Filled') return OrderStatus.Closed;
  if (['PartialCanceled', 'Canceled', 'Rejected'].includes(value)) return OrderStatus.Canceled;
  throw new Error('Unknown TxFlow order status');
}

export class TxflowAccount {
  readonly orderIds = new Map<string, string>();
  private generation = 0;
  private stateRevision = 0;
  private ordersRevision = 0;
  private settingsRevision = 0;
  private stateTime = 0;
  private orderTimes = new Map<string, number>();
  private orderMemo: string[] = [];
  private memoIds = new Set<string>();
  private terminal = new Set<string>();
  private terminalQueue: string[] = [];
  private inflight = new Set<string>();
  private seenFills = new Set<number>();
  private fillsExhausted = false;
  private startupAt = 0;
  private lastRefresh = -Infinity;
  private lastSettings = -Infinity;
  private started = false;
  private settings = new Map<string, FetchedLeverage>();
  private settingsQueue: string[] = [];
  private settingTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly cx: Context) {}

  start = async (): Promise<void> => {
    if (this.started) return;
    this.started = true;
    this.startupAt = Math.max(0, Math.floor(Date.now()));
    this.cx.store.setSetting('isHedged', false);
    await this.refresh();
  };

  tick = async (): Promise<void> => {
    if (!this.started || Date.now() - this.lastRefresh < 5000) return;
    await this.refresh();
  };

  refresh = async (): Promise<void> => {
    if (!this.started || this.cx.disposed()) return;
    this.lastRefresh = Date.now();
    const jobs = [this.refreshState(), this.refreshOrders(), this.refreshFills()];
    if (this.settingsQueue.length === 0 && Date.now() - this.lastSettings >= 60_000) {
      this.lastSettings = Date.now();
      this.settingsQueue = this.cx.store.markets.map((m) => m.id);
      jobs.push(this.nextSetting());
    }
    await Promise.all(jobs);
  };

  disconnected = (): void => {
    this.generation += 1;
    this.inflight.clear();
    this.settingsQueue = [];
    this.lastSettings = -Infinity;
    if (this.settingTimer) clearTimeout(this.settingTimer);
    this.settingTimer = undefined;
  };

  dispose = (): void => {
    this.started = false;
    this.disconnected();
  };

  writeAccepted = (): void => {
    this.stateRevision += 1;
    this.ordersRevision += 1;
    this.settingsRevision += 1;
    void this.refresh();
  };

  confirmedSetting = (symbol: string): FetchedLeverage | undefined => this.settings.get(symbol);

  confirmSetting = (symbol: string, leverage: number, marginType: MarginType): void => {
    this.settingsRevision += 1;
    this.applySettings([[symbol, { buyLeverage: leverage, sellLeverage: leverage, marginType }]]);
  };

  private alive(generation: number): boolean {
    return this.started && !this.cx.disposed() && generation === this.generation;
  }

  private subject(user: string | undefined): void {
    if (user !== undefined && user.toLowerCase() !== this.cx.wallet().toLowerCase()) {
      throw new Error('Mismatched TxFlow account subject');
    }
  }

  private read = <T>(type: string, fields: Record<string, unknown> = {}): Promise<T> =>
    this.cx.requestInfo<T>({ type, user: this.cx.wallet(), ...fields });

  private mapState = (native: NativeState): { balance: Balance; positions: Position[] } => {
    this.subject(native.user);
    if (!Array.isArray(native.assetPositions)) throw new Error('Missing TxFlow positions');
    const equity = num(native.marginSummary.accountValue);
    const used = num(native.marginSummary.totalMarginUsed);
    const positions: Position[] = [];
    for (const { position: p, type } of native.assetPositions) {
      const market = this.cx.marketById(p.asset);
      if (!market) throw new Error('Unknown TxFlow position asset');
      const leverage = num(p.leverage.value);
      if (leverage <= 0) throw new Error('Invalid TxFlow position leverage');
      const marginType = mode(p.leverage.type);
      const size = num(p.szi);
      if (!['oneway', 'hedge'].includes(type.toLowerCase())) throw new Error('Unknown TxFlow position mode');
      if (size === 0) continue;
      const side = size > 0 ? PositionSide.Long : PositionSide.Short;
      positions.push({
        id: `${market.symbol}:${side}`,
        symbol: market.symbol,
        side,
        entryPrice: p.entryPx == null ? 0 : num(p.entryPx),
        notional: Math.abs(num(p.positionValue)),
        leverage,
        marginType,
        positionMode: type.toLowerCase() === 'hedge' ? PositionMode.Hedged : PositionMode.OneWay,
        unrealizedPnl: num(p.unrealizedPnl),
        contracts: Math.abs(size),
        liquidationPrice: p.liquidationPx == null ? 0 : num(p.liquidationPx),
        marginSize: num(p.marginUsed),
        ...(p.updateTime == null ? {} : { lastUpdate: num(p.updateTime) }),
      });
    }
    return { balance: { total: equity, used, free: equity - used, upnl: equity - num(native.marginSummary.totalRawUsd) }, positions };
  };

  fetchBalance = async (): Promise<Balance> => this.mapState(await this.read<NativeState>('clearinghouseState', { accountType: 'perp' })).balance;
  fetchPositions = async (): Promise<Position[]> => this.mapState(await this.read<NativeState>('clearinghouseState', { accountType: 'perp' })).positions;

  private refreshState = async (): Promise<void> => {
    if (this.inflight.has('state')) return;
    this.inflight.add('state');
    const generation = this.generation;
    const revision = this.stateRevision;
    const modeRevision = this.settingsRevision;
    try {
      const native = await this.read<NativeState>('clearinghouseState', { accountType: 'perp' });
      const mapped = this.mapState(native);
      const time = native.time === undefined ? undefined : num(native.time);
      if (!this.alive(generation) || revision !== this.stateRevision || (time !== undefined && time < this.stateTime)) return;
      this.commitState(mapped, time, modeRevision === this.settingsRevision);
    } catch (e) {
      if (this.alive(generation)) this.cx.reportError(e instanceof Error ? e : new Error('TxFlow account read failed'));
    } finally {
      if (generation === this.generation) this.inflight.delete('state');
    }
  };

  private commitState(mapped: ReturnType<TxflowAccount['mapState']>, time: number | undefined, applyMode: boolean): void {
    if (time !== undefined) this.stateTime = time;
    this.cx.store.update({ balance: mapped.balance, loaded: { ...this.cx.store.loaded, balance: true, positions: true } });
    if (applyMode && mapped.positions[0]) this.cx.store.setSetting('isHedged', mapped.positions[0].positionMode === PositionMode.Hedged);
    this.cx.store.addOrUpdatePositionsFull(mapped.positions);
  }

  private mapOrder = (native: NativeOrder, state: string, at: number): Order => {
    const market = this.cx.marketById(id(native.asset));
    if (!market) throw new Error('Unknown TxFlow order asset');
    const amount = num(native.origSz);
    const remaining = num(native.sz);
    if (amount < 0 || remaining < 0 || remaining > amount) throw new Error('Invalid TxFlow order size');
    if (!['B', 'A'].includes(native.side)) throw new Error('Unknown TxFlow order side');
    let type: OrderType;
    if (native.isTrigger) {
      const trigger = (native.triggerType ?? '').toLowerCase();
      type = ['tp', 'takeprofit'].includes(trigger) ? OrderType.TakeProfit : ['sl', 'stoploss'].includes(trigger) ? OrderType.StopLoss : OrderType.Conditional;
    } else if (native.orderType === 'Limit') type = OrderType.Limit;
    else if (native.orderType === 'Market') type = OrderType.Market;
    else throw new Error('Unknown TxFlow order type');
    const tifs: Record<string, OrderTimeInForce> = {
      GTC: OrderTimeInForce.GoodTillCancel, Gtc: OrderTimeInForce.GoodTillCancel,
      IOC: OrderTimeInForce.ImmediateOrCancel, Ioc: OrderTimeInForce.ImmediateOrCancel,
      FOK: OrderTimeInForce.FillOrKill, Fok: OrderTimeInForce.FillOrKill,
      ALO: OrderTimeInForce.PostOnly, Alo: OrderTimeInForce.PostOnly, PostOnly: OrderTimeInForce.PostOnly,
    };
    if (native.tif && !tifs[native.tif]) throw new Error('Unknown TxFlow order tif');
    return {
      id: id(native.oid), symbol: market.symbol, status: status(state), type,
      side: native.side === 'B' ? OrderSide.Buy : OrderSide.Sell,
      price: native.isTrigger && native.triggerPx != null ? num(native.triggerPx) : num(native.limitPx),
      amount, remaining, filled: amount - remaining, reduceOnly: native.reduceOnly ?? false,
      ...(native.cloid == null ? {} : { clientOrderId: native.cloid }),
      ...(native.tif ? { timeInForce: tifs[native.tif] } : {}),
      lastUpdate: Math.max(at, num(native.timestamp)),
      info: { childrenKnown: Object.hasOwn(native, 'children') && (native.children == null || native.children.length === 0) },
    };
  };

  fetchOrders = async (): Promise<Order[]> => {
    const orders: Order[] = [];
    const ids = new Set<string>();
    for (let offset = 0; offset <= 200_000; offset += PAGE) {
      const rows = await this.read<NativeOrder[]>('frontendOpenOrders', { offset, limit: PAGE });
      if (!Array.isArray(rows)) throw new Error('Malformed TxFlow open orders');
      for (const row of rows) {
        if (row.state === undefined) throw new Error('Missing TxFlow order state');
        const order = this.mapOrder(row, row.state, num(row.timestamp));
        if (ids.has(order.id)) throw new Error('Duplicate TxFlow order ID');
        ids.add(order.id);
        orders.push(order);
      }
      if (rows.length < PAGE) return orders;
    }
    throw new Error('TxFlow open-order coverage exhausted');
  };

  refreshOrders = async (): Promise<void> => {
    if (this.inflight.has('orders')) return;
    this.inflight.add('orders');
    const generation = this.generation;
    const revision = this.ordersRevision;
    try {
      const orders = await this.fetchOrders();
      if (!this.alive(generation) || revision !== this.ordersRevision) return;
      for (const order of orders) this.rememberOrder(order);
      this.cx.store.addOrUpdateOrdersFull(orders.filter((o) => !this.terminal.has(o.id)));
      this.cx.store.update({ loaded: { ...this.cx.store.loaded, orders: true } });
      this.trimOrderMemo();
      this.cx.onOrders?.();
    } catch (e) {
      if (this.alive(generation)) this.cx.reportError(e instanceof Error ? e : new Error('TxFlow orders read failed'));
    } finally {
      if (generation === this.generation) this.inflight.delete('orders');
    }
  };

  private refreshFills = async (): Promise<void> => {
    if (this.inflight.has('fills') || this.fillsExhausted) return;
    this.inflight.add('fills');
    const generation = this.generation;
    try {
      const fills: NativeFill[] = [];
      const ids = new Set<number>();
      for (let offset = 0; ; offset += PAGE) {
        if (offset > 200_000) throw new Error('TxFlow execution coverage exhausted');
        const page = await this.read<{ data: NativeFill[]; hasMore: boolean }>('userFills', {
          accountType: ['perp'], offset, limit: PAGE, startTimeMs: this.startupAt,
        });
        if (!Array.isArray(page.data) || typeof page.hasMore !== 'boolean') throw new Error('Malformed TxFlow execution page');
        for (const fill of page.data) {
          id(fill.tid); id(fill.oid); id(fill.instrumentId);
          if (num(fill.px) <= 0 || num(fill.sz) <= 0 || num(fill.time) < 0 || !['B', 'A'].includes(fill.side)) throw new Error('Malformed TxFlow execution');
          if (!ids.has(fill.tid)) { ids.add(fill.tid); fills.push(fill); }
        }
        if (!page.hasMore) break;
        if (fills.length > FILL_CAP) throw new Error('TxFlow execution coverage exhausted');
      }
      if (!this.alive(generation)) return;
      if (this.seenFills.size + fills.filter((f) => !this.seenFills.has(f.tid)).length > FILL_CAP) {
        this.fillsExhausted = true;
        throw new Error('TxFlow execution deduplication capacity exhausted; restart account reconciliation');
      }
      for (const fill of fills.reverse()) {
        if (this.seenFills.has(fill.tid)) continue;
        this.seenFills.add(fill.tid);
        if (num(fill.time) < this.startupAt) continue;
        const market = this.cx.marketById(String(fill.instrumentId));
        if (market) this.cx.emitFill({ id: String(fill.oid), symbol: market.symbol, side: fill.side === 'B' ? OrderSide.Buy : OrderSide.Sell, price: num(fill.px), amount: num(fill.sz) });
      }
    } catch (e) {
      if (this.alive(generation)) this.cx.reportError(e instanceof Error ? e : new Error('TxFlow execution read failed'));
    } finally {
      if (generation === this.generation) this.inflight.delete('fills');
    }
  };

  private mapAsset(data: AssetData, expected?: string): [string, FetchedLeverage] {
    this.subject(data.user);
    if (expected !== undefined && data.coin !== expected) throw new Error('Mismatched TxFlow setting asset');
    const market = this.cx.marketById(data.coin);
    if (!market) throw new Error('Unknown TxFlow setting asset');
    return [market.symbol, setting(data.leverage)];
  }

  private applySettings(settings: Array<[string, FetchedLeverage]>): void {
    if (settings.length === 0) return;
    const fetchedLeverage = { ...this.cx.store.fetchedLeverage };
    for (const [symbol, s] of settings) {
      if (s.positionMode !== undefined) this.cx.store.setSetting('isHedged', s.positionMode === PositionMode.Hedged);
      this.settings.set(symbol, s); fetchedLeverage[symbol] = s;
    }
    this.cx.store.update({ fetchedLeverage });
  }

  private nextSetting = async (): Promise<void> => {
    if (this.inflight.has('setting') || this.settingTimer || this.cx.disposed()) return;
    const coin = this.settingsQueue.shift();
    if (coin === undefined) return;
    const generation = this.generation;
    const revision = this.settingsRevision;
    this.inflight.add('setting');
    try {
      const data = await this.read<AssetData>('activeAssetData', { coin });
      if (this.alive(generation) && revision === this.settingsRevision) {
        const mapped = this.mapAsset(data, coin);
        this.settingsRevision += 1;
        this.applySettings([mapped]);
      }
    } catch (e) {
      if (this.alive(generation)) this.cx.reportError(e instanceof Error ? e : new Error('TxFlow setting read failed'));
    } finally {
      if (generation === this.generation) {
        this.inflight.delete('setting');
        this.settingTimer = setTimeout(() => { this.settingTimer = undefined; void this.nextSetting(); }, 100);
      }
    }
  };

  private rememberOrder(order: Order): void {
    if (!this.memoIds.has(order.id)) { this.memoIds.add(order.id); this.orderMemo.push(order.id); }
    if (order.clientOrderId !== undefined) this.orderIds.set(order.clientOrderId, order.id);
  }

  private trimOrderMemo(): void {
    const live = new Set(this.cx.store.orders.map((o) => o.id));
    while (this.orderMemo.length > 16_384 + live.size) {
      const oldest = this.orderMemo.shift()!;
      if (live.has(oldest)) { this.orderMemo.push(oldest); continue; }
      this.memoIds.delete(oldest);
      this.orderTimes.delete(oldest);
      for (const [cloid, oid] of this.orderIds) if (oid === oldest) this.orderIds.delete(cloid);
    }
  }

  nodeFrame = (text: string): void => {
    if (!this.started || this.cx.disposed()) return;
    try {
      const frame = JSON.parse(text) as { channel: string; data: unknown };
      if (frame.channel === 'clearinghouseState') {
        const native = frame.data as NativeState;
        const mapped = this.mapState(native);
        const time = native.time === undefined ? undefined : num(native.time);
        if (time !== undefined && time < this.stateTime) return;
        this.stateRevision += 1;
        this.settingsRevision += 1;
        this.commitState(mapped, time, true);
      } else if (frame.channel === 'orderUpdates') {
        const updates = frame.data as Array<{ order: NativeOrder; status: string; statusTimestamp: Numeric }>;
        const orders = updates.map((u) => this.mapOrder(u.order, u.status, num(u.statusTimestamp)));
        this.ordersRevision += 1;
        for (const order of orders) {
          if ((this.orderTimes.get(order.id) ?? -Infinity) > order.lastUpdate || this.terminal.has(order.id)) continue;
          this.orderTimes.set(order.id, order.lastUpdate);
          this.rememberOrder(order);
          if (order.status === OrderStatus.Closed || order.status === OrderStatus.Canceled) {
            this.terminal.add(order.id);
            this.terminalQueue.push(order.id);
            this.cx.store.markOrdersTerminal([{ id: order.id }], order.status);
            this.cx.store.removeOrder({ id: order.id });
          } else this.cx.store.addOrUpdateOrder(order);
        }
        while (this.terminalQueue.length > 16_384) {
          const oldest = this.terminalQueue.shift()!;
          this.terminal.delete(oldest);
        }
        this.trimOrderMemo();
      this.cx.onOrders?.();
        void this.refresh();
      } else if (frame.channel === 'activeAssetData') {
        const mapped = this.mapAsset(frame.data as AssetData);
        this.settingsRevision += 1;
        this.applySettings([mapped]);
      } else if (frame.channel === 'userAssetData') {
        const data = frame.data as { user: string; coins: AssetData[] };
        this.subject(data.user);
        const mapped = data.coins.map((c) => this.mapAsset(c));
        this.settingsRevision += 1;
        this.applySettings(mapped);
      }
    } catch {
      // A malformed push cannot partially replace any account scope.
    }
  };

  apiFrame = (_text: string): void => {
    // /api-ws userFills is a historical snapshot, never a live execution feed.
  };
}
