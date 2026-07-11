import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import {
  LogSeverity,
  MarginType,
  Market,
  Order,
  OrderSide,
  OrderStatus,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  Writable,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { systemSubscriptionTypes } from './hyperliquid.constants';
import { hasSeenFill, markFillSeen } from './hyperliquid.cross-cache';
import { HyperliquidExchange } from './hyperliquid.exchange';
import { inferHyperliquidOrderTypeAndPrice, parseHyperliquidOrderTimeInForce } from './hyperliquid.order-inference';
import { UserFills, UserFundingEntry, UserNonFundingLedgerEntry, WebData2, WSS_URLS } from './hyperliquid.types';

// Per-channel timestamp tracking for deduplication
interface PerChannelTimestampState {
  lastTime: Map<string, number>; // channel -> last timestamp
  lastDataHash: Map<string, string>; // channel -> hash for non-serverTime data (activeAssetData)
}

export class HyperliquidPrivateWebSocket extends BaseWebSocket<HyperliquidExchange> {
  // Independent ping loop
  private hlPingTimeoutId?: ReturnType<typeof setTimeout>;

  // Per-channel timestamp tracking state
  private timestampState: PerChannelTimestampState = {
    lastTime: new Map(),
    lastDataHash: new Map(),
  };

  // Core-perp clearinghouseState is the primary source of truth for positions.
  // webData2 snapshots can be stale and must only be used as fallback.
  private lastCoreClearinghouseStateAt = 0;
  private lastCoreClearinghouseStateServerTime = 0;
  private static readonly WEB_DATA2_CORE_POSITION_FALLBACK_MS = 20_000;
  private static readonly MAX_TRACKED_REALIZED_PNL_FILL_IDS = 20_000;
  private static readonly CORE_FILL_POSITION_EPSILON = 1e-10;
  private realizedPnlByPositionKey = new Map<string, number>();
  private processedRealizedPnlFillIds = new Set<string>();
  private processedRealizedPnlFillQueue: string[] = [];

  constructor(parent: HyperliquidExchange) {
    super(parent);
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION']);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.subscribe();
      this.startPingLoop();
    }
  };

  private startPingLoop = () => {
    // Clear any existing timeout
    this.stopPingLoop();
    // Don't send ping immediately on connect - wait for first scheduled ping
    // Sending ping before server responds to subscription can cause issues
    this.schedulePing();
  };

  // Ping interval: 15 seconds (CCXT uses 20s, our previous 1s was too aggressive)
  private static readonly PING_INTERVAL_MS = 15_000;

  // Channels that should skip the timestamp-stale check (event-driven,
  // every message matters). Hoisted to class scope as a Set so the hot
  // dispatch path doesn't allocate a fresh array + run `.includes()`
  // (linear scan) per WS frame.
  private static readonly SKIP_TIMESTAMP_CHECK_CHANNELS: ReadonlySet<string> = new Set([
    'userFills',
    'userFundings',
    'userNonFundingLedgerUpdates',
    'notification',
    'orderUpdates',
    'userEvents',
  ]);

  private schedulePing = () => {
    this.hlPingTimeoutId = setTimeout(() => {
      if (!this.isDisposed && this.isConnected) {
        this.ping();
        this.schedulePing();
      }
    }, HyperliquidPrivateWebSocket.PING_INTERVAL_MS);
  };

  private stopPingLoop = () => {
    if (this.hlPingTimeoutId) {
      clearTimeout(this.hlPingTimeoutId);
      this.hlPingTimeoutId = undefined;
    }
  };

  subscribe = () => {
    this.subscribeSystemTopics();
    // Subscribe to activeAssetData for all markets (leverage/margin info)
    this.subscribeAllActiveAssetData();
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ method: 'ping' }));
    }
  };

  subscribeSystemTopics = () => {
    const user = this.parent.options.vault || this.parent.options.applicationId;

    for (const type of systemSubscriptionTypes) {
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type,
            user,
          },
        }),
      );
    }

    if (this.parent.shouldUseSpotCollateralBalance()) {
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type: 'spotClearinghouseState',
            user,
          },
        }),
      );
    }

    // Core-perp clearinghouseState subscription for real-time position updates.
    this.send(
      JSON.stringify({
        method: 'subscribe',
        subscription: {
          type: 'clearinghouseState',
          user,
        },
      }),
    );

    // Subscribe to HIP-3 DEX topics
    // For HIP-3 (builder-deployed assets), we need dex parameter on each channel
    // See: https://hyperliquid.gitbook.io/hyperliquid-docs
    for (const dexName of this.parent.hip3DexNames) {
      // allMids with dex is the reliable HIP-3 last-price stream.
      // webData2 with dex can fall back to core-perp payloads on some paths.
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type: 'allMids',
            dex: dexName,
          },
        }),
      );
      // clearinghouseState with dex for faster real-time position updates
      // This is the authoritative position snapshot for the DEX.
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type: 'clearinghouseState',
            user,
            dex: dexName,
          },
        }),
      );
      // userFills with dex for instant HIP-3 fill notifications
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type: 'userFills',
            user,
            dex: dexName,
          },
        }),
      );
      // orderUpdates with dex for incremental order status changes
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type: 'orderUpdates',
            user,
            dex: dexName,
          },
        }),
      );
      // userEvents with dex for comprehensive order lifecycle events
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type: 'userEvents',
            user,
            dex: dexName,
          },
        }),
      );
    }
  };

  /**
   * Subscribe to activeAssetData for all markets (leverage/margin info)
   * Skips HIP-3 markets as they may not be supported or could exceed subscription limits
   */
  private subscribeAllActiveAssetData = () => {
    const user = this.parent.options.vault || this.parent.options.applicationId;
    const markets = this.parent.store.markets;

    for (const market of markets) {
      const coin = market.id;
      // Skip HIP-3 markets (contain ':') to avoid exceeding 1000 channel limit
      if (coin.includes(':')) continue;
      this.send(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type: 'activeAssetData',
            user,
            coin,
          },
        }),
      );
    }
  };

  private getStaleCheckChannelKey(channel: string, data: any): string {
    // Keep timestamp streams separate per HIP-3 dex so updates from one dex
    // do not incorrectly suppress another dex's updates.
    if ((channel === 'clearinghouseState' || channel === 'webData2') && data?.dex) {
      return `${channel}:${String(data.dex).toLowerCase()}`;
    }
    return channel;
  }

  private getMessageTimestamp(data: any): number | null {
    // Hyperliquid payloads are inconsistent:
    // - some channels use data.serverTime
    // - clearinghouseState commonly uses data.time
    // - webData2 may include nested clearinghouseState.time
    const rawTimestamp = data?.serverTime ?? data?.time ?? data?.clearinghouseState?.time;
    const timestamp = Number(rawTimestamp);
    if (!Number.isFinite(timestamp) || timestamp <= 0) {
      return null;
    }
    return timestamp;
  }

  /**
   * Extract a usable core-perp balance summary from clearinghouseState payload.
   * Some sources expose marginSummary, others crossMarginSummary.
   */
  private extractCoreBalanceSummary(
    payload: any,
  ): { accountValue: number; totalMarginUsed: number; sourceField: 'marginSummary' | 'crossMarginSummary' } | null {
    const sourceField = payload?.marginSummary
      ? 'marginSummary'
      : payload?.crossMarginSummary
        ? 'crossMarginSummary'
        : null;
    if (!sourceField) {
      return null;
    }

    const accountValue = Number(payload?.[sourceField]?.accountValue);
    const totalMarginUsed = Number(payload?.[sourceField]?.totalMarginUsed);
    if (!Number.isFinite(accountValue) || !Number.isFinite(totalMarginUsed)) {
      return null;
    }

    return {
      accountValue,
      totalMarginUsed,
      sourceField,
    };
  }

  private collectHip3DexesFromFills(fills: UserFills[]): Set<string> {
    const dexes = new Set<string>();
    const marketsById = this.parent.store.marketsById;

    for (const fill of fills) {
      const coin = String(fill?.coin || '');
      if (!coin) continue;

      if (coin.includes(':')) {
        const [dexPrefix] = coin.split(':');
        if (dexPrefix) {
          dexes.add(dexPrefix.toLowerCase());
        }
        continue;
      }

      const market = marketsById[coin];
      const marketDex = market?.info?.hip3Dex;
      if (typeof marketDex === 'string' && marketDex) {
        dexes.add(marketDex.toLowerCase());
      }
    }

    return dexes;
  }

  /**
   * Hyperliquid position payloads can omit `type` on some streams (notably HIP-3).
   * Treat missing/unknown values as one-way to keep IDs stable and avoid duplicate
   * symbol vs symbol:side entries for the same logical position.
   */
  private resolvePositionMode(assetPosition: any): PositionMode {
    const rawType = String(assetPosition?.type || '').toLowerCase();
    if (rawType === 'twoway' || rawType === 'hedged') {
      return PositionMode.Hedged;
    }
    return PositionMode.OneWay;
  }

  // Check if message is stale based on serverTime or data hash
  private isStaleMessage(json: any): boolean {
    const channel = json.channel;
    const data = json.data;

    if (!channel || !data) return false;

    // Use channel timestamps when available
    const messageTime = this.getMessageTimestamp(data);
    if (messageTime !== null) {
      const channelKey = this.getStaleCheckChannelKey(channel, data);
      const lastTime = this.timestampState.lastTime.get(channelKey) || 0;

      if (messageTime <= lastTime) {
        return true;
      }
      this.timestampState.lastTime.set(channelKey, messageTime);
      return false;
    }

    // For activeAssetData, use coin + a quick hash (no serverTime available)
    if (channel === 'activeAssetData' && data.coin) {
      const key = `${channel}:${data.coin}`;
      const hash = `${data.markPx || ''}:${data.leverage?.value || ''}`;
      const lastHash = this.timestampState.lastDataHash.get(key);
      if (hash === lastHash) {
        return true;
      }
      this.timestampState.lastDataHash.set(key, hash);
      return false;
    }

    return false;
  }

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);

        // Handle pong separately (no deduplication needed)
        if (json?.channel === 'pong') {
          this.recordMessageHandlerStats('pong');
          // Just record latency, don't schedule next ping (handled by interval)
          if (this.pingAt) {
            const diff = performance.now() - this.pingAt;
            this.store.update({ latency: Math.round(diff / 2) });
          }
          return;
        }

        // Check for stale messages (skip for channels that need all messages).
        // Hoisted as a class-level Set: the prior shape allocated a fresh
        // 6-element array per WS frame and ran `.includes()` (linear scan).
        if (json?.channel && !HyperliquidPrivateWebSocket.SKIP_TIMESTAMP_CHECK_CHANNELS.has(json.channel)) {
          if (this.isStaleMessage(json)) return;
        }

        // Don't track subscriptionResponse/subscribed channels in stats
        if (
          json?.channel !== 'subscriptionResponse' &&
          json?.channel !== 'SubscriptionResponse' &&
          json?.channel !== 'subscribed'
        ) {
          this.recordMessageHandlerStats(json?.channel);
        }
        this.processMessage(json);
      } catch (_e) {}
    }
  };

  // Unified message processing for both WebSocket sources
  private processMessage(json: any) {
    switch (json?.channel) {
      case 'allMids':
        this.handleHip3AllMids(json?.data || {});
        break;
      case 'activeAssetData':
        if (json?.data?.coin) {
          const coin = json?.data?.coin;
          this.parent.activeAssetData.set(coin, json?.data || {});

          // Use O(1) hashmap lookup instead of O(n) find
          const market = this.store.marketsById[coin];
          if (market) {
            // Hot path: indexed scan with `continue` instead of
            // `positions.filter(...)` — filter allocates a fresh array
            // per frame even when no position matches.
            const allPositions = this.store.positions;
            const symbolConfiguration = this.parent._getSymbolConfigurationFromAssetData(coin);
            for (let i = 0; i < allPositions.length; i++) {
              const position = allPositions[i];
              if (position.symbol !== market.symbol) continue;
              // activeAssetData can lag behind clearinghouseState and may temporarily
              // carry stale leverage values. For open positions, keep leverage/marginType
              // from position streams to avoid UI snapbacks. For empty/fake positions,
              // apply symbol configuration so pre-position leverage defaults stay current.
              //
              // Pass ONLY the fields we want to change. The prior shape
              // spread `...position` into the update — that copied every
              // position field into a fresh object just to write it back
              // unchanged onto the same live position.
              const isOpenPosition = position.contracts > 0;
              this.parent.store.updatePosition(position, {
                positionMode: symbolConfiguration.positionMode,
                marginType: isOpenPosition ? position.marginType : symbolConfiguration.marginType,
                leverage: isOpenPosition ? position.leverage : symbolConfiguration.leverage,
              });
            }
          }
        }
        break;
      case 'webData2':
        this.handleWebData2(json?.data || {});
        break;
      case 'error':
        this.handleServerError(json?.data);
        break;
      case 'spotClearinghouseState':
        this.parent.handleSpotClearinghouseStateUpdate(json?.data?.balances || []);
        break;
      case 'clearinghouseState':
        if (json?.data) {
          // Hyperliquid wraps both core and HIP-3 subscription payloads as
          // `{ user, dex, clearinghouseState }`. Core uses `dex: ''`, so use
          // the nested state before routing it to the core handler.
          const chState = json.data.clearinghouseState || json.data;
          if (json.data.dex) {
            // HIP-3 clearinghouseState with dex parameter — route through handleWebData2
            // which already handles per-dex position preservation correctly.
            // The dex channel may send data either as:
            //   { dex: "xyz", assetPositions: [...] }
            //   { dex: "xyz", clearinghouseState: { assetPositions: [...] } }
            // Normalize to the format handleWebData2 expects.
            this.handleWebData2({
              clearinghouseState: {
                assetPositions: chState.assetPositions,
                marginSummary: chState.marginSummary,
              },
              dex: json.data.dex,
            });
          } else {
            // Core perps clearinghouseState (no dex)
            this.applyCoreClearinghouseState(chState);
          }
        }
        break;
      case 'userFills':
        this.handleFills(json?.data || {});
        break;
      case 'userFundings':
        this.handleFundings(json?.data || {});
        break;
      case 'userNonFundingLedgerUpdates':
        this.handleUserNonFundingLedgerUpdates(json?.data || {});
        break;
      case 'orderUpdates':
        this.handleOrderUpdates(json?.data || []);
        break;
      case 'userEvents':
        this.handleUserEvents(json?.data);
        break;
      default:
        break;
    }
  }

  // Reusable update buffer + same optimizations as the public WS's
  // `handleAllMids`: for-in over the mids object (no `Object.entries`
  // tuple-array alloc), single shared `{ last }` buffer instead of
  // per-ticker allocs, skip the `Set` allocation when there are no
  // positions to sync.
  private _hip3MidsUpdateBuf: { last: number; midPrice: number } = {
    last: 0,
    midPrice: 0,
  };

  private handleHip3AllMids(data: any): void {
    const mids = data?.mids;
    if (!mids || typeof mids !== 'object') return;

    const tickersById = this.parent.store.tickersById;
    const hasPositions = this.parent.store.positions.length > 0;
    const updatedSymbols: Set<string> | undefined = hasPositions ? new Set<string>() : undefined;
    const updateBuf = this._hip3MidsUpdateBuf;

    for (const coin in mids) {
      if (!coin.includes(':')) continue;
      const ticker = tickersById[coin];
      if (!ticker) continue;
      const last = Number(mids[coin]);
      if (!Number.isFinite(last)) continue;
      // Hyperliquid's `allMids` IS the canonical mid (`midPx`).
      // Populate both `last` and `midPrice` so `$mid` reads the
      // published mid, not the impact-derived `(bid+ask)/2`.
      updateBuf.last = last;
      updateBuf.midPrice = last;
      this.parent.store.updateTicker(ticker, updateBuf);
      if (updatedSymbols) updatedSymbols.add(ticker.symbol);
    }

    if (updatedSymbols && updatedSymbols.size > 0) {
      this.parent.syncBalanceFromLivePositions(updatedSymbols);
    }
  }

  private handleUserEvents = (data: any): void => {
    if (!data) return;

    const user = data?.user || this.parent.options.vault || this.parent.options.applicationId || '';

    const routeFills = (fills: any, isSnapshot = false) => {
      if (!Array.isArray(fills) || fills.length === 0) return;
      const normalizedFills = fills.filter(
        (entry) =>
          entry && typeof entry === 'object' && 'tid' in entry && 'coin' in entry && 'px' in entry && 'sz' in entry,
      );
      if (normalizedFills.length === 0) return;
      this.handleFills({ fills: normalizedFills as UserFills[], isSnapshot, user });
    };

    const routeOrders = (orders: any) => {
      if (!Array.isArray(orders) || orders.length === 0) return;
      this.handleOrderUpdates(orders);
    };

    if (Array.isArray(data)) {
      const fillEvents = data.filter(
        (entry) =>
          entry && typeof entry === 'object' && 'tid' in entry && 'coin' in entry && 'px' in entry && 'sz' in entry,
      );
      if (fillEvents.length > 0) {
        routeFills(fillEvents);
        return;
      }

      // Some userEvents payloads carry order updates as an array of update objects.
      routeOrders(data);
      return;
    }

    routeFills(data?.fills, Boolean(data?.isSnapshot));
    routeOrders(data?.orders);
  };

  private handleServerError(data: unknown): void {
    // Hyperliquid includes the complete rejected subscription in its error
    // text, including the public wallet address. Keep diagnostics useful
    // without forwarding account identifiers or raw protocol frames.
    const subscriptionType =
      typeof data === 'string' ? data.match(/"type"\s*:\s*"([^\"]+)"/)?.[1] : undefined;
    this.parent.log(
      `Hyperliquid WebSocket server error${subscriptionType ? `: rejected ${subscriptionType} subscription` : ''}`,
      LogSeverity.Warning,
    );
  }

  /**
   * Apply core-perp account state from either the private WebSocket or the
   * authoritative REST reconciliation. REST snapshots replace core positions
   * in full so a position closed while disconnected is removed; streaming
   * frames remain incremental because Hyperliquid can scope them.
   */
  public applyCoreClearinghouseState(data: any, authoritative = false): void {
    const serverTime = this.getMessageTimestamp(data);
    if (serverTime !== null) {
      if (serverTime < this.lastCoreClearinghouseStateServerTime) return;
      this.lastCoreClearinghouseStateServerTime = serverTime;
    }

    if (data?.assetPositions) {
      this.lastCoreClearinghouseStateAt = Date.now();

      // Use O(1) hashmap lookup instead of O(n) find in map.
      // Indexed for-loop with `continue` instead of `.map(...).filter(...)`
      // — that pattern allocated a result array sized to the input
      // (including the nulls), then a second array for the filtered
      // result. Single push-into-corePositions does the same job in
      // one allocation.
      const marketsById = this.parent.store.marketsById;
      const assetPositions = data.assetPositions;
      const corePositions: Position[] = [];
      for (let i = 0; i < assetPositions.length; i++) {
        const assetPosition = assetPositions[i];
        const market = marketsById[assetPosition.position.coin];
        if (!market) continue;
        const positionMode = this.resolvePositionMode(assetPosition);
        const side = +assetPosition.position.szi > 0 ? PositionSide.Long : PositionSide.Short;
        const realizedPnl = this.getRealizedPnlForPosition(market.symbol, side);
        corePositions.push({
          // Spread defaults first, then explicit API values override them
          ...this.parent._getSymbolConfigurationFromAssetData(market.id),
          id: createPositionId(market.symbol, side, positionMode),
          symbol: market.symbol,
          side,
          entryPrice: +assetPosition.position.entryPx,
          notional: +assetPosition.position.positionValue,
          leverage: +assetPosition.position.leverage.value,
          unrealizedPnl: +assetPosition.position.unrealizedPnl,
          ...(realizedPnl !== undefined ? { realizedPnl } : {}),
          contracts: Math.abs(+assetPosition.position.szi),
          liquidationPrice: +assetPosition.position.liquidationPx,
          marginSize: assetPosition.position.marginUsed != null ? +assetPosition.position.marginUsed : undefined,
          positionMode,
          marginType: assetPosition.position.leverage.type === 'cross' ? MarginType.Cross : MarginType.Isolated,
        });
      }

      if (authoritative) {
        // A REST clearinghouseState response is a complete core-perp snapshot.
        // Preserve HIP-3 rows, then replace all core rows (including the empty
        // response case) while retaining fake rows for order-entry defaults.
        const merged = new Map<string, Position>();
        const fakes = this.parent.getFakePositions();
        for (let i = 0; i < fakes.length; i++) merged.set(fakes[i].id, fakes[i]);

        const currentPositions = this.parent.store.positions;
        for (let i = 0; i < currentPositions.length; i++) {
          const position = currentPositions[i];
          const market = this.parent.store.marketsBySymbol[position.symbol];
          if (market?.info?.hip3Dex) merged.set(position.id, position);
        }

        for (let i = 0; i < corePositions.length; i++) {
          merged.set(corePositions[i].id, corePositions[i]);
        }
        this.parent.store.addOrUpdatePositionsFull(Array.from(merged.values()));
      } else {
        // Core clearinghouseState updates can arrive more frequently and may be
        // partial/scoped. Merge incrementally to avoid wiping symbol leverage
        // to fake/default values between updates.
        this.parent.store.addOrUpdatePositions(corePositions);
      }
    }

    const balanceSummary = this.extractCoreBalanceSummary(data);
    if (balanceSummary) {
      const total = balanceSummary.accountValue;
      const used = balanceSummary.totalMarginUsed;
      // Indexed for-loop instead of `positions.reduce(...)` — drops the
      // closure allocation per clearinghouseState frame.
      const positions = this.store.positions;
      let upnl = 0;
      for (let i = 0; i < positions.length; i++) {
        upnl += positions[i].unrealizedPnl;
      }
      this.parent.applyCorePerpsBalanceSnapshot({
        accountValue: total,
        totalMarginUsed: used,
        upnl,
      });
    }

    this.parent.syncBalanceFromLivePositions();
  }

  dispose = () => {
    this.stopPingLoop();
    this.ws?.close?.();
  };

  // Reusable update buffer for webData2 ticker fields. webData2 frames
  // arrive ~0.4–50/sec depending on dex count; the prior shape allocated
  // a fresh `Partial<Ticker>` per ticker per frame (up to ~600 allocs/frame
  // × N dex subscriptions). The buffer always carries the same 9 fields,
  // so the store's `updateTicker` for-in iterates and writes them onto
  // the live ticker — no reference is retained, so reuse is safe.
  private _webData2UpdateBuf: Partial<Writable<Ticker>> = {
    mark: 0,
    last: 0,
    midPrice: 0,
    index: 0,
    bid: 0,
    ask: 0,
    openInterest: 0,
    fundingRate: 0,
    volume: 0,
  };

  handleWebData2 = (data: WebData2) => {
    // Skip the `Set` allocation when there are no positions to sync —
    // `syncBalanceFromLivePositions` bails fast in that case anyway.
    const hasPositions = this.parent.store.positions.length > 0;
    const updatedSymbols: Set<string> | undefined = hasPositions ? new Set<string>() : undefined;

    if (data?.meta?.universe && Array.isArray(data?.meta?.universe)) {
      try {
        // handle market data changes
        // Use O(1) hashmap lookup instead of O(n) find
        const tickersById = this.parent.store.tickersById;
        const universe = data.meta.universe;
        const assetCtxs = data?.assetCtxs;
        const updateBuf = this._webData2UpdateBuf;
        for (let i = 0; i < universe.length; i++) {
          const asset = assetCtxs?.[i];
          if (!asset) continue;
          const ticker = tickersById[universe[i].name];
          if (!ticker) continue;

          const midPx = parseFloat(asset.midPx);
          updateBuf.mark = parseFloat(asset.markPx);
          updateBuf.last = midPx;
          updateBuf.midPrice = midPx;
          updateBuf.index = parseFloat(asset.oraclePx);
          updateBuf.bid = parseFloat(asset.impactPxs?.[0] || asset.midPx);
          updateBuf.ask = parseFloat(asset.impactPxs?.[1] || asset.midPx);
          updateBuf.openInterest = parseFloat(asset.openInterest);
          updateBuf.fundingRate = parseFloat(asset.funding);
          updateBuf.volume = parseFloat(asset.dayNtlVlm);

          this.store.updateTicker(ticker, updateBuf);
          if (updatedSymbols) updatedSymbols.add(ticker.symbol);
        }
      } catch (e) {
        this.parent.log(`Error processing ticker update: ${e}`, LogSeverity.Error);
      }
    }

    if (updatedSymbols && updatedSymbols.size > 0) {
      this.parent.syncBalanceFromLivePositions(updatedSymbols);
    }

    if (data?.clearinghouseState?.assetPositions) {
      const messageDex = (data as any)?.dex?.toLowerCase();
      const isHip3Message =
        !!messageDex ||
        (data?.clearinghouseState?.assetPositions || []).some((ap: any) => ap.position?.coin?.includes(':'));

      // For core perps, prefer dedicated clearinghouseState stream.
      // webData2 position snapshots are periodic and can re-introduce stale leverage.
      const hasRecentCoreClearinghouseState =
        this.lastCoreClearinghouseStateAt > 0 &&
        Date.now() - this.lastCoreClearinghouseStateAt <
          HyperliquidPrivateWebSocket.WEB_DATA2_CORE_POSITION_FALLBACK_MS;
      if (!isHip3Message && hasRecentCoreClearinghouseState) {
        // Skip only position replacement from core webData2 while primary stream is healthy.
      } else {
        // Use O(1) hashmap lookup instead of O(n) find in map
        const marketsById2 = this.parent.store.marketsById;

        // This handler receives clearinghouseState from both core perps (no-dex)
        // and HIP-3 (official API with dex). Core perps come without colon in coin names.
        // Indexed for-loop with `continue` instead of `.map(...).filter(...)`.
        const rawAssetPositions = data?.clearinghouseState?.assetPositions || [];
        const incomingPositions: Position[] = [];
        for (let i = 0; i < rawAssetPositions.length; i++) {
          const assetPosition = rawAssetPositions[i];
          // Try exact match first
          let market: Market | undefined = marketsById2[assetPosition.position.coin];
          // If not found and coin has colon (HIP-3 format), try case-insensitive
          if (!market && assetPosition.position.coin.includes(':')) {
            market = this.parent.marketsLowerCaseMap.get(assetPosition.position.coin.toLowerCase());
          }
          if (!market) continue;

          const positionMode = this.resolvePositionMode(assetPosition);
          const side = +assetPosition.position.szi > 0 ? PositionSide.Long : PositionSide.Short;
          const realizedPnl = this.getRealizedPnlForPosition(market.symbol, side);

          incomingPositions.push({
            // Spread defaults first, then explicit API values override them
            ...this.parent._getSymbolConfigurationFromAssetData(market.id),
            id: createPositionId(market.symbol, side, positionMode),
            symbol: market.symbol,
            side,
            entryPrice: +assetPosition.position.entryPx,
            notional: +assetPosition.position.positionValue,
            leverage: +assetPosition.position.leverage.value,
            unrealizedPnl: +assetPosition.position.unrealizedPnl,
            ...(realizedPnl !== undefined ? { realizedPnl } : {}),
            contracts: Math.abs(+assetPosition.position.szi),
            liquidationPrice: +assetPosition.position.liquidationPx,
            marginSize: assetPosition.position.marginUsed != null ? +assetPosition.position.marginUsed : undefined,
            positionMode,
            marginType: assetPosition.position.leverage.type === 'cross' ? MarginType.Cross : MarginType.Isolated,
          });
        }

        // Detect if this message is from a HIP-3 dex or core perps.
        // Use data.dex as the primary signal — it's always present for HIP-3 messages
        // (set by the dex subscription or by our clearinghouseState routing).
        // Coin content detection is a fallback only. This is critical because when a
        // HIP-3 position is CLOSED, assetPositions is empty and .some() returns false,
        // which would incorrectly treat the message as core perp data and preserve
        // the closed position in the store.
        if (isHip3Message) {
          // Preserve positions from:
          // 1. Core perps (always)
          // 2. Other HIP-3 DEXes (not this one)
          const positionsToPreserve = this.parent.store.positions.filter((p) => {
            const market = this.parent.store.marketsBySymbol[p.symbol];
            const positionDex = market?.info?.hip3Dex?.toLowerCase();

            if (!positionDex) {
              // Core perp position - always preserve
              return true;
            } else {
              // HIP-3 position - preserve only if different DEX
              return positionDex !== messageDex;
            }
          });

          // Merge via Map instead of `[...spread, ...spread, ...spread]`
          // + `uniqBy(arr, 'id')`. The prior shape allocated a triple-
          // spread array of all three sources (~610 entries on a typical
          // HIP-3 account: ~5 incoming + ~5 preserve + ~600 fake),
          // then `uniqBy` allocated a deduped result + internal Set.
          //
          // `uniqBy` keeps the FIRST occurrence; `Map.set` keeps the
          // LAST. To match the original semantic (incoming wins over
          // preserve wins over fake), insert in REVERSE precedence:
          // fakes first (lowest), preserve second, incoming last.
          const fakes = this.parent.getFakePositions();
          const merged = new Map<string, Position>();
          for (let i = 0; i < fakes.length; i++) merged.set(fakes[i].id, fakes[i]);
          for (let i = 0; i < positionsToPreserve.length; i++) {
            merged.set(positionsToPreserve[i].id, positionsToPreserve[i]);
          }
          for (let i = 0; i < incomingPositions.length; i++) {
            merged.set(incomingPositions[i].id, incomingPositions[i]);
          }
          this.parent.store.addOrUpdatePositionsFull(Array.from(merged.values()));
        } else {
          // This is core perp data coming from webData2 fallback.
          //
          // webData2 snapshots can be partial/stale while core clearinghouseState is degraded.
          // Using full replacement here can transiently zero live core positions, which causes
          // visible UI row/account flapping until the next authoritative update arrives.
          //
          // Merge incrementally instead; position removals are still handled by the authoritative
          // core clearinghouseState stream and periodic reconciliation paths.
          if (incomingPositions.length > 0) {
            this.parent.store.addOrUpdatePositions(incomingPositions);
          }
        }
      }
    }

    // IMPORTANT:
    // Do NOT update global balance from webData2 snapshots.
    //
    // webData2 can be stale and can also arrive from HIP-3 subscriptions where
    // payload shape is not consistently tagged. In practice this can overwrite
    // a valid core-perp balance with zero when the user is flat.
    //
    // Global balance is sourced exclusively from handleClearinghouseState()
    // (core clearinghouseState stream), with startup REST fetch as initial seed.

    if (data?.openOrders) {
      // Use O(1) hashmap lookup instead of O(n) find in map
      const marketsById3 = this.parent.store.marketsById;

      // Detect whether this webData2 message is from a HIP-3 dex or core perps.
      // This mirrors the same detection used in the clearinghouseState section above.
      const messageDex = (data as any)?.dex?.toLowerCase();
      const isHip3Snapshot = !!messageDex;

      // Pre-build orderId → existingOrder hashmap once. The prior shape
      // ran `store.orders.find(o => o.id === orderId)` per openOrder
      // (potentially twice — once for HIP-3 type preservation, once for
      // tif fallback). For ~50 orders in the snapshot × ~50 stored, that
      // was ~5K linear comparisons per webData2 frame.
      const existingOrdersById: Record<string, Order> = {};
      const storeOrders = this.parent.store.orders;
      for (let i = 0; i < storeOrders.length; i++) {
        existingOrdersById[storeOrders[i].id] = storeOrders[i];
      }

      // Parse incoming orders from the snapshot. Single indexed for-loop
      // replaces filter().map().filter(...). The prior chain allocated
      // three intermediate arrays + two closure pools; this builds the
      // result directly via push with `continue` for the skip cases.
      const rawOpenOrders = data.openOrders;
      const newOrders: Order[] = [];
      for (let i = 0; i < rawOpenOrders.length; i++) {
        const openOrder = rawOpenOrders[i];
        const oid = `${openOrder.oid}`;
        // Filter out dead orders (from updateOrder cancel+replace)
        if (this.parent._deadOrders.has(oid)) continue;
        // Filter out recently cancelled orders
        if (this.parent.isOrderCancelled(oid)) continue;

        // Try exact match first, then case-insensitive for HIP-3 markets
        let market: Market | undefined = marketsById3[openOrder.coin];
        if (!market && openOrder.coin.includes(':')) {
          market = this.parent.marketsLowerCaseMap.get(openOrder.coin.toLowerCase());
        }
        if (!market) continue;

        // Single O(1) lookup feeds both the HIP-3 type-preservation path
        // and the timeInForce fallback below.
        const existingOrder = existingOrdersById[oid];
        let inferredType: OrderType;
        let inferredPrice: number;

        // For HIP-3 orders, check if already in store (from frontendOpenOrders)
        const isHip3 = market.id.includes(':');

        if (isHip3 && existingOrder) {
          // Preserve order type from store
          inferredType = existingOrder.type;
          inferredPrice = existingOrder.price;
        } else if (isHip3 && !openOrder.orderType && !openOrder.triggerPx && openOrder.limitPx) {
          // HIP-3 order not in store - try pre-placement cache with WS data for disambiguation
          const side = openOrder.side === 'B' ? OrderSide.Buy : OrderSide.Sell;
          const wsLimitPx = parseFloat(openOrder.limitPx) || undefined;
          const wsAmount = openOrder.origSz ? +openOrder.origSz : undefined;
          const prePlacement = this.parent.matchPrePlacement(openOrder.coin, side, wsLimitPx, wsAmount);
          if (prePlacement) {
            inferredType = prePlacement.type;
            inferredPrice = prePlacement.price;
          } else {
            // Not in any cache - skip and let REST add it correctly
            this.parent._hip3OrdersNeedRefresh = true;
            continue;
          }
        } else {
          const inferred = inferHyperliquidOrderTypeAndPrice(openOrder);
          inferredType = inferred.type;
          inferredPrice = inferred.price;
        }

        // Determine timeInForce: use WS data if present, fall back to store value.
        // webData2 openOrders may omit tif for some orders — preserve from store to prevent
        // PostOnly being lost on full array replacement.
        const tif = parseHyperliquidOrderTimeInForce(openOrder.tif) || existingOrder?.timeInForce;

        newOrders.push({
          id: oid,
          status: OrderStatus.Open,
          symbol: market.symbol,
          type: inferredType,
          side: openOrder.side === 'B' ? OrderSide.Buy : OrderSide.Sell,
          price: inferredPrice,
          amount: +openOrder.origSz,
          filled: +openOrder.origSz - +openOrder.sz,
          remaining: +openOrder.sz,
          reduceOnly: Boolean(openOrder.reduceOnly),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          timeInForce: tif,
          info: openOrder,
        });
      }

      // Build the set of new order IDs so we can skip duplicates when
      // preserving. Direct loop avoids the intermediate `.map(o => o.id)`
      // array that the prior shape allocated just to feed Set's iterator.
      const newOrderIds = new Set<string>();
      for (let i = 0; i < newOrders.length; i++) newOrderIds.add(newOrders[i].id);

      if (isHip3Snapshot) {
        // HIP-3 dex webData2: this snapshot only contains orders for ONE dex.
        // Treat as ADDITIVE ONLY — can add/update orders but never remove them.
        // webData2 snapshots are periodic (~0.4/s) and inherently stale. They may
        // not include orders that were already confirmed by the real-time orderUpdates
        // channel. Removing confirmed orders based on a stale snapshot causes visible
        // flicker (orders vanish for 1-2s until the next snapshot or orderUpdates restores them).
        // Order removal is handled by authoritative channels:
        //   - orderUpdates WS: real-time (canceled, filled, rejected)
        //   - fetchOrders() REST: source of truth (120s safety net)
        const ordersToPreserve = this.parent.store.orders.filter((order) => {
          if (this.parent.isOrderCancelled(order.id)) return false;
          if (newOrderIds.has(order.id)) return false; // Already in snapshot — use snapshot version
          return true; // Preserve everything else
        });

        this.parent.store.addOrUpdateOrdersFull([...newOrders, ...ordersToPreserve]);
      } else {
        // Core perps webData2: this snapshot contains all core perp orders.
        // Preserve existing HIP-3 orders (they come from dex-specific channels).
        // Also preserve optimistic core perp orders not yet in the snapshot.
        const existingHip3Orders = this.parent.store.orders.filter((order) => {
          if (this.parent.isOrderCancelled(order.id)) return false;
          if (newOrderIds.has(order.id)) return false; // Already in snapshot — use snapshot version

          // Always preserve optimistic orders not yet confirmed by WS
          const isOptimistic = order.status === OrderStatus.Creating || order.status === OrderStatus.Updating;
          if (isOptimistic) return true;

          const market = this.parent.store.marketsBySymbol[order.symbol];
          return market?.id?.includes(':') || order.symbol.includes('|');
        });

        this.parent.store.addOrUpdateOrdersFull([...newOrders, ...existingHip3Orders]);
      }
    }
  };

  handleFills = (data: { fills: UserFills[]; isSnapshot: boolean; user: string }) => {
    const fills = data.fills || [];
    if (!fills.length) return;

    // Use O(1) hashmap lookup instead of O(n) find fallback
    const marketsById = this.parent.store.marketsById;
    const sortedFills = fills
      .slice()
      .sort((a, b) => Number(a.time || 0) - Number(b.time || 0) || Number(a.tid || 0) - Number(b.tid || 0));

    // Indexed for-loop over `forEach` — drops iterator + closure
    // overhead. Snapshot loads can carry hundreds of fills.
    for (let fillIdx = 0; fillIdx < sortedFills.length; fillIdx++) {
      const fill = sortedFills[fillIdx];
      // Deduplicate fills by transaction ID to prevent duplicate toast notifications
      // Uses SHARED deduplication across all instances (sessionStorage + broadcast)
      const fillId = `${fill.tid}`;
      const fillDir = String(fill.dir || '').toLowerCase();
      const fillPositionSide = this.getFillPositionSide(fillDir);
      const fee = this.safeParseNumber(fill.fee);
      const closedPnl = this.safeParseNumber(fill.closedPnl);
      const netRealizedPnl = closedPnl - fee;

      // Keep a local accumulator that is seeded from snapshots and updated by live fills.
      // This avoids expensive history backfills while making realized PnL live-updating.
      if (!this.processedRealizedPnlFillIds.has(fillId)) {
        this.trackProcessedRealizedPnlFill(fillId);

        if (fillPositionSide && fillDir.includes('open') && Math.abs(this.safeParseNumber(fill.startPosition)) === 0) {
          // New lifecycle for this side/symbol starts here.
          this.setRealizedPnlForPosition(fill.coin, fillPositionSide, 0, marketsById);
        }

        if (fillPositionSide && fillDir.includes('close')) {
          this.incrementRealizedPnlForPosition(fill.coin, fillPositionSide, netRealizedPnl, marketsById);
          this.applyRealizedPnlToCurrentPosition(fill.coin, fillPositionSide, marketsById);
        }
      }

      if (data.isSnapshot) continue;

      // Always attempt local optimistic reconciliation first.
      // Cross-context dedupe is only for duplicate fill notifications.
      this.applyOptimisticCorePositionFromFill(fill, marketsById);

      // Check shared deduplication first (cross-instance)
      if (hasSeenFill(fillId)) continue;

      // Mark as seen in shared storage (broadcasts to other instances)
      markFillSeen(fillId);

      // Try exact match first, then case-insensitive for HIP-3 markets (O(1) map lookup)
      let market: Market | undefined = marketsById[fill.coin];
      if (!market && fill.coin.includes(':')) {
        // Try case-insensitive lookup for HIP-3 markets via O(1) map
        market = this.parent.marketsLowerCaseMap.get(fill.coin.toLowerCase());
      }
      const symbol = market?.symbol || fill.coin;

      this.parent.emitter.emit('fill', {
        id: fillId,
        orderId: `${fill.oid}`, // Include order ID for chaser fill tracking
        side: fill.side === 'B' ? OrderSide.Buy : OrderSide.Sell,
        symbol,
        price: parseFloat(fill.px),
        amount: parseFloat(fill.sz),
        ...(fillPositionSide && fillDir.includes('close') ? { realizedPnl: netRealizedPnl, fee } : {}),
      });
    }

    // Keep a debounced REST safety-net for HIP-3 fills.
    // This prevents stale chart/store positions when dex clearinghouseState is delayed/missed.
    if (!data.isSnapshot) {
      const hip3Dexes = this.collectHip3DexesFromFills(sortedFills);
      if (hip3Dexes.size > 0) {
        this.parent.scheduleHip3PositionRefresh(hip3Dexes);
      }
    }
  };

  private applyOptimisticCorePositionFromFill(fill: UserFills, marketsById: Record<string, Market>): void {
    // Core perps only (HIP-3 uses dex-scoped clearinghouseState reconciliation).
    if (!fill?.coin || fill.coin.includes(':')) return;

    const fillPrice = this.safeParseNumber(fill.px);
    const fillSize = this.safeParseNumber(fill.sz);
    const startPosition = Number(fill.startPosition);
    if (!Number.isFinite(fillPrice) || fillPrice <= 0 || !Number.isFinite(fillSize) || fillSize <= 0) return;
    // startPosition is required for deterministic optimistic reconciliation.
    // Treat missing/invalid values as non-actionable instead of coercing to zero.
    if (!Number.isFinite(startPosition)) return;

    // Hyperliquid uses "B" for buys and non-B values (commonly "A") for sells.
    // Match the rest of our handlers: treat anything other than "B" as sell.
    const side = String(fill.side || '').toUpperCase();
    const signedFillSize = side === 'B' ? fillSize : -fillSize;

    // Try exact match first, then case-insensitive lookup.
    let market: Market | undefined = marketsById[fill.coin];
    if (!market) {
      market = this.parent.marketsLowerCaseMap.get(fill.coin.toLowerCase());
    }
    if (!market || market.id.includes(':')) return;

    let nextSignedPosition = startPosition + signedFillSize;
    if (Math.abs(nextSignedPosition) < HyperliquidPrivateWebSocket.CORE_FILL_POSITION_EPSILON) {
      nextSignedPosition = 0;
    }

    const nextContracts = Math.abs(nextSignedPosition);
    const nextSide = nextSignedPosition < 0 ? PositionSide.Short : PositionSide.Long;
    const existingPosition = this.store.positions.find((position) => position.symbol === market.symbol);
    const symbolConfig = this.parent._getSymbolConfigurationFromAssetData(market.id);

    // This optimistic path is for Hyperliquid one-way core perps.
    // If hedged mode is ever introduced, defer to authoritative position streams.
    if ((existingPosition?.positionMode || symbolConfig.positionMode) === PositionMode.Hedged) return;

    if (!existingPosition && nextContracts === 0) return;

    if (
      existingPosition &&
      Math.abs(existingPosition.contracts - nextContracts) < HyperliquidPrivateWebSocket.CORE_FILL_POSITION_EPSILON &&
      existingPosition.side === nextSide
    ) {
      return;
    }

    const positionMode = existingPosition?.positionMode || PositionMode.OneWay;
    const leverage = existingPosition?.leverage || symbolConfig.leverage;
    const marginType = existingPosition?.marginType || symbolConfig.marginType;

    let nextEntryPrice = 0;
    if (nextContracts > 0) {
      const prevContracts = Math.max(Math.abs(startPosition), 0);
      const sameDirection =
        Math.sign(startPosition) === Math.sign(nextSignedPosition) && Math.sign(startPosition) !== 0;
      const isAddingToPosition = sameDirection && nextContracts > prevContracts;
      const hasUsableExistingEntry =
        !!existingPosition && Number.isFinite(existingPosition.entryPrice) && existingPosition.entryPrice > 0;

      if (!hasUsableExistingEntry) {
        nextEntryPrice = fillPrice;
      } else if (isAddingToPosition && prevContracts > 0) {
        nextEntryPrice =
          (existingPosition.entryPrice * prevContracts + fillPrice * Math.abs(signedFillSize)) / nextContracts;
      } else if (sameDirection && nextContracts <= prevContracts) {
        nextEntryPrice = existingPosition.entryPrice;
      } else {
        // Position flipped or opened from flat; reset entry to the fill price.
        nextEntryPrice = fillPrice;
      }
    }

    const optimisticPosition: Position = {
      ...(existingPosition || {
        id: createPositionId(market.symbol, nextSide, positionMode),
        symbol: market.symbol,
        side: nextSide,
        entryPrice: 0,
        notional: 0,
        leverage,
        unrealizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
        positionMode,
        marginType,
      }),
      id: createPositionId(market.symbol, nextSide, positionMode),
      symbol: market.symbol,
      side: nextSide,
      entryPrice: nextEntryPrice,
      notional: nextContracts === 0 ? 0 : nextContracts * fillPrice,
      leverage,
      marginType,
      positionMode,
      unrealizedPnl: nextContracts === 0 ? 0 : existingPosition?.unrealizedPnl || 0,
      contracts: nextContracts,
      liquidationPrice: nextContracts === 0 ? 0 : existingPosition?.liquidationPrice || 0,
    };

    this.parent.store.addOrUpdatePosition(optimisticPosition);
  }

  handleFundings = (_data: { fundings: UserFundingEntry[]; isSnapshot: boolean; user: string }) => {};

  handleUserNonFundingLedgerUpdates = (_data: {
    nonFundingLedgerUpdates: UserNonFundingLedgerEntry[];
    isSnapshot: boolean;
    user: string;
  }) => {
    if (!this.parent.shouldUseSpotCollateralBalance()) return;
    this.parent.requestUnifiedSpotCollateralRefresh();
  };

  /**
   * Handle orderUpdates channel - incremental order status updates
   * This is the main channel for receiving order changes including HIP-3 orders
   */
  handleOrderUpdates = (updates: any[]) => {
    if (!Array.isArray(updates) || updates.length === 0) return;

    const marketsById = this.parent.store.marketsById;
    // Pre-build orderId → existingOrder hashmap once for the whole
    // batch. The HIP-3 paths below would otherwise run
    // `store.orders.find(o => o.id === orderId)` per update — once for
    // type preservation, once for the tif fallback. For ~50 batched
    // updates × ~50 stored orders that's ~5K linear comparisons.
    const storeOrders = this.parent.store.orders;
    const existingOrdersById: Record<string, Order> = {};
    for (let i = 0; i < storeOrders.length; i++) {
      existingOrdersById[storeOrders[i].id] = storeOrders[i];
    }

    for (const update of updates) {
      const orderData = update?.order;
      if (!orderData) continue;

      // Try exact match first, then case-insensitive for HIP-3 markets (O(1) map lookup)
      let market: Market | undefined = marketsById[orderData.coin];
      if (!market && orderData.coin.includes(':')) {
        // Try case-insensitive lookup for HIP-3 markets via O(1) map
        market = this.parent.marketsLowerCaseMap.get(orderData.coin.toLowerCase());
        if (market) {
          this.parent.log(
            `[orderUpdates] Found market via case-insensitive lookup: coin="${orderData.coin}" matched market.id="${market.id}"`,
            LogSeverity.Info,
          );
        }
      }

      if (!market) {
        this.parent.log(
          `[orderUpdates] Market not found for order coin="${orderData.coin}". Available HIP-3 markets: ${Object.keys(
            marketsById,
          )
            .filter((k) => k.includes(':'))
            .slice(0, 5)
            .join(', ')}...`,
          LogSeverity.Warning,
        );
        continue;
      }

      const orderId = `${orderData.oid}`;
      const status = update.status; // 'open', 'filled', 'canceled', 'triggered', 'rejected', 'marginCanceled'

      this.parent.log(
        `[orderUpdates] Processing order update: coin=${orderData.coin}, symbol=${market.symbol}, oid=${orderId}, status=${status}`,
        LogSeverity.Debug,
      );

      // Handle order removal (filled, canceled, etc.)
      // reduceOnlyCanceled: TP/SL orders auto-cancelled when position is closed
      // reduceOnlyRejected: reduce-only order rejected (position already closed)
      if (
        status === 'filled' ||
        status === 'canceled' ||
        status === 'rejected' ||
        status === 'marginCanceled' ||
        status === 'reduceOnlyCanceled' ||
        status === 'reduceOnlyRejected'
      ) {
        this.parent.store.removeOrder({ id: orderId });
        continue;
      }

      // Handle open/triggered orders - add or update
      // Skip if this order was recently cancelled — stale WS snapshots can arrive
      // after a cancel succeeds, and re-adding the order confuses the user.
      // REST backfill (fetchOrders) is the source of truth for reconciliation.
      if (this.parent.isOrderCancelled(orderId)) {
        this.parent.log(
          `[orderUpdates] Suppressing stale open status for recently cancelled order: oid=${orderId}`,
          LogSeverity.Debug,
        );
        continue;
      }

      if (status === 'open' || status === 'triggered') {
        // Infer order type - HIP-3 orders don't include orderType/triggerPx/tpsl fields
        // First check cache for HIP-3 orders (we store type when placing orders)
        // Core perps include orderType explicitly
        let inferredType: OrderType;
        let inferredPrice: number;
        let inferenceSource = 'unknown';

        const isHip3 = market.id.includes(':');

        // For HIP-3 orders, check pending cache first (just placed, shared across instances)
        const pendingOrder = isHip3 ? this.parent.getPendingHip3Order(orderId) : null;

        if (pendingOrder) {
          // Use cached type/price from order we just placed (from shared cache)
          inferredType = pendingOrder.type;
          inferredPrice = pendingOrder.price;
          inferenceSource = 'pending-cache';
          // Remove from shared pending cache now that we've used it
          this.parent.removePendingHip3Order(orderId);
          // Also consume the corresponding pre-placement entry to prevent staleness.
          // Pre-placement entries are added BEFORE REST, pending entries AFTER REST.
          // If we don't consume here, old pre-placement entries accumulate and can
          // poison future matchPrePlacement calls (e.g., after cancel-all + re-place).
          const side = orderData.side === 'B' ? OrderSide.Buy : OrderSide.Sell;
          this.parent.matchPrePlacement(orderData.coin, side);
        } else if (isHip3 && !orderData.orderType && !orderData.triggerPx && orderData.limitPx) {
          // HIP-3 order without explicit type — check store (O(1) via the
          // pre-built map), then pre-placement cache.
          const existingOrder = existingOrdersById[orderId];

          if (existingOrder) {
            // Preserve order type/price from store (set by frontendOpenOrders or optimistic add)
            // Still process the WS update to transition status Creating/Updating → Open
            inferredType = existingOrder.type;
            inferredPrice = existingOrder.price;
            if (existingOrder.status === OrderStatus.Creating || existingOrder.status === OrderStatus.Updating) {
              inferenceSource = 'optimistic-preserved';
            } else {
              inferenceSource = 'store-preserved';
            }
          } else {
            // Try pre-placement cache with WS data for disambiguation
            const side = orderData.side === 'B' ? OrderSide.Buy : OrderSide.Sell;
            const wsLimitPx = parseFloat(orderData.limitPx) || undefined;
            const wsAmount = orderData.origSz ? +orderData.origSz : undefined;
            const prePlacement = this.parent.matchPrePlacement(orderData.coin, side, wsLimitPx, wsAmount);
            if (prePlacement) {
              // Found in pre-placement cache - WS arrived before REST returned
              inferredType = prePlacement.type;
              inferredPrice = prePlacement.price;
              inferenceSource = 'pre-placement-cache';
            } else {
              // No match in any cache — add placeholder so user sees something
              // Using Creating status signals the UI that this order is still syncing
              inferredType = OrderType.Limit;
              inferredPrice = parseFloat(orderData.limitPx) || 0;
              inferenceSource = 'hip3-unconfirmed-placeholder';
              // Trigger debounced REST refresh (500ms) to get correct order type.
              // This is common during early init when no caches are populated yet.
              this.parent.scheduleOrderRefresh();
            }
          }
        } else {
          const inferred = inferHyperliquidOrderTypeAndPrice(orderData);
          inferredType = inferred.type;
          inferredPrice = inferred.price;
          inferenceSource = inferred.source;
        }

        // Log inference source for HIP-3 orders to aid debugging type/price mismatches
        if (isHip3) {
          this.parent.log(
            `[orderUpdates] HIP-3 order type inference: oid=${orderId}, source=${inferenceSource}, type=${inferredType}, price=${inferredPrice}, wsLimitPx=${orderData.limitPx}`,
            LogSeverity.Debug,
          );
        }

        // Use Creating status for unconfirmed HIP-3 placeholders to indicate they're still syncing
        // This tells the UI not to allow full interaction until REST confirms the order type
        const isUnconfirmedPlaceholder = inferenceSource === 'hip3-unconfirmed-placeholder';
        const orderStatus = isUnconfirmedPlaceholder ? OrderStatus.Creating : OrderStatus.Open;

        // Determine timeInForce: use WS data if present, otherwise preserve from store.
        // HIP-3 orderUpdates may omit tif, so we fall back to the existing order's value
        // (set by optimistic add or REST frontendOpenOrders).
        let inferredTimeInForce = parseHyperliquidOrderTimeInForce(orderData.tif);
        if (!inferredTimeInForce && isHip3) {
          // HIP-3 WS may omit tif — preserve from existing order in store
          // (O(1) via the pre-built map).
          inferredTimeInForce = existingOrdersById[orderId]?.timeInForce;
        }

        const order: Order = {
          id: orderId,
          status: orderStatus,
          symbol: market.symbol,
          type: inferredType,
          side: orderData.side === 'B' ? OrderSide.Buy : OrderSide.Sell,
          price: inferredPrice,
          amount: +orderData.origSz,
          filled: +orderData.origSz - +orderData.sz,
          remaining: +orderData.sz,
          reduceOnly: Boolean(orderData.reduceOnly),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          timeInForce: inferredTimeInForce,
          info: orderData,
        };

        this.parent.store.addOrUpdateOrder(order);
      }
    }
  };

  private getFillPositionSide(fillDir: string): PositionSide | undefined {
    if (fillDir.includes('long')) return PositionSide.Long;
    if (fillDir.includes('short')) return PositionSide.Short;
    return undefined;
  }

  private safeParseNumber(value: unknown): number {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : 0;
  }

  private trackProcessedRealizedPnlFill(fillId: string) {
    this.processedRealizedPnlFillIds.add(fillId);
    this.processedRealizedPnlFillQueue.push(fillId);

    if (this.processedRealizedPnlFillQueue.length > HyperliquidPrivateWebSocket.MAX_TRACKED_REALIZED_PNL_FILL_IDS) {
      const oldestFillId = this.processedRealizedPnlFillQueue.shift();
      if (oldestFillId) {
        this.processedRealizedPnlFillIds.delete(oldestFillId);
      }
    }
  }

  private getRealizedPnlKey(symbol: string, side: PositionSide) {
    return `${symbol}:${side}`;
  }

  private resolveSymbolFromCoin(coin: string, marketsById: Record<string, Market>): string {
    let market: Market | undefined = marketsById[coin];
    if (!market && coin.includes(':')) {
      market = this.parent.marketsLowerCaseMap.get(coin.toLowerCase());
    }
    return market?.symbol || coin;
  }

  private setRealizedPnlForPosition(
    coin: string,
    side: PositionSide,
    value: number,
    marketsById: Record<string, Market>,
  ): void {
    const symbol = this.resolveSymbolFromCoin(coin, marketsById);
    this.realizedPnlByPositionKey.set(this.getRealizedPnlKey(symbol, side), value);
  }

  private incrementRealizedPnlForPosition(
    coin: string,
    side: PositionSide,
    delta: number,
    marketsById: Record<string, Market>,
  ): void {
    const symbol = this.resolveSymbolFromCoin(coin, marketsById);
    const key = this.getRealizedPnlKey(symbol, side);
    this.realizedPnlByPositionKey.set(key, (this.realizedPnlByPositionKey.get(key) || 0) + delta);
  }

  private applyRealizedPnlToCurrentPosition(
    coin: string,
    side: PositionSide,
    marketsById: Record<string, Market>,
  ): void {
    const symbol = this.resolveSymbolFromCoin(coin, marketsById);
    const realizedPnl = this.getRealizedPnlForPosition(symbol, side);
    if (realizedPnl === undefined) return;

    const currentPosition = this.store.positions.find(
      (position) => position.symbol === symbol && position.side === side,
    );
    if (!currentPosition) return;

    this.parent.store.updatePosition(currentPosition, { realizedPnl });
  }

  private getRealizedPnlForPosition(symbol: string, side: PositionSide): number | undefined {
    return this.realizedPnlByPositionKey.get(this.getRealizedPnlKey(symbol, side));
  }
}
