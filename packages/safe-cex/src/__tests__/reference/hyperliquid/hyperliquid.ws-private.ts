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
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { readHyperliquidCloid } from '../../../exchanges/hyperliquid/hyperliquid.cloid';
import { systemSubscriptionTypes } from '../../../exchanges/hyperliquid/hyperliquid.constants';
import { hasSeenFill, markFillSeen } from './hyperliquid.cross-cache';
import { HyperliquidExchange } from './hyperliquid.exchange';
import { inferHyperliquidOrderTypeAndPrice, parseHyperliquidOrderTimeInForce } from '../../../exchanges/hyperliquid/hyperliquid.order-inference';
import { UserFills, UserFundingEntry, UserNonFundingLedgerEntry, WSS_URLS } from '../../../exchanges/hyperliquid/hyperliquid.types';
import {
  formatHyperliquidWsDiagnostic,
  shouldEmitHyperliquidWsReceiveDiagnostic,
  shouldEmitHyperliquidWsSendDiagnostic,
} from './hyperliquid.ws-diagnostics';

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
  private lastCoreClearinghouseStateAt = 0;
  private lastCoreClearinghouseStateServerTime = 0;
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
      this.sendWithDiagnostics(JSON.stringify({ method: 'ping' }));
    }
  };

  subscribeSystemTopics = () => {
    const user = this.parent.options.vault || this.parent.options.applicationId;

    for (const type of systemSubscriptionTypes) {
      this.sendWithDiagnostics(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type,
            user,
          },
        }),
      );
    }

    // Opt-in standard accounts need this too — they use spot state for the
    // holdings display, not as collateral.
    if (this.parent.shouldTrackSpotHoldings()) {
      this.sendWithDiagnostics(
        JSON.stringify({
          method: 'subscribe',
          subscription: {
            type: 'spotState',
            user,
            ...(this.parent.shouldRequestPortfolioMarginSpotState() ? { isPortfolioMargin: true } : {}),
          },
        }),
      );
    }

    // Core-perp clearinghouseState subscription for real-time position updates.
    this.sendWithDiagnostics(
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
      this.sendWithDiagnostics(
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
      this.sendWithDiagnostics(
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
      this.sendWithDiagnostics(
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
      this.sendWithDiagnostics(
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
      this.sendWithDiagnostics(
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
   * Skips HIP-3 and spot markets as they may not be supported or could exceed
   * subscription limits
   */
  private subscribeAllActiveAssetData = () => {
    const user = this.parent.options.vault || this.parent.options.applicationId;
    const markets = this.parent.store.markets;

    for (const market of markets) {
      const coin = market.id;
      // Skip spot: activeAssetData carries leverage/margin-mode, which is
      // meaningless for spot (always 1x, no margin mode). Spot roughly
      // doubles the market count, so subscribing would burn ~300 channels
      // against the 1000 limit for data we'd discard. Note this must be an
      // `isSpot` check, NOT a name heuristic — the canonical spot pair is
      // literally named `PURR/USDC` and would slip past the HIP-3 ':' guard.
      if (market.isSpot) continue;
      // Skip HIP-3 markets (contain ':') to avoid exceeding 1000 channel limit
      if (coin.includes(':')) continue;
      this.sendWithDiagnostics(
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
    if (channel === 'clearinghouseState' && data?.dex) {
      return `${channel}:${String(data.dex).toLowerCase()}`;
    }
    return channel;
  }

  private getMessageTimestamp(data: any): number | null {
    // Hyperliquid payloads are inconsistent:
    // - some channels use data.serverTime
    // - clearinghouseState commonly uses data.time
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
        this.emitReceiveDiagnostics(data, json);

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

  private sendWithDiagnostics = (data: string) => {
    this.send(data);
    if (shouldEmitHyperliquidWsSendDiagnostic(data)) {
      this.emitWsDiagnosticSend(data, {
        format: (message) => formatHyperliquidWsDiagnostic('private', message),
      });
    }
  };

  private emitReceiveDiagnostics = (data: unknown, json: unknown) => {
    if (shouldEmitHyperliquidWsReceiveDiagnostic('private', json)) {
      this.emitWsDiagnosticReceive(data, {
        format: (message) => formatHyperliquidWsDiagnostic('private', message),
      });
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
      case 'error':
        this.handleServerError(json?.data);
        break;
      case 'spotState':
      case 'spotClearinghouseState':
        this.parent.handleSpotClearinghouseStateUpdate(
          json?.data?.spotState || json?.data || { balances: [] },
          'websocket',
        );
        break;
      case 'clearinghouseState':
        if (json?.data) {
          // Hyperliquid wraps both core and HIP-3 subscription payloads as
          // `{ user, dex, clearinghouseState }`. Core uses `dex: ''`, so use
          // the nested state before routing it to the core handler.
          const chState = json.data.clearinghouseState || json.data;
          if (json.data.dex) {
            this.handleHip3ClearinghouseState(json.data.dex, chState);
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
    const subscriptionType = typeof data === 'string' ? data.match(/"type"\s*:\s*"([^\"]+)"/)?.[1] : undefined;
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

  handleHip3ClearinghouseState = (dex: string, data: { assetPositions?: any[] }) => {
    const messageDex = String(dex || '').toLowerCase();
    if (!messageDex || !Array.isArray(data?.assetPositions)) return;

    const marketsById = this.parent.store.marketsById;
    const incomingPositions: Position[] = [];
    for (let i = 0; i < data.assetPositions.length; i++) {
      const assetPosition = data.assetPositions[i];
      const coin = String(assetPosition?.position?.coin || '');
      let market: Market | undefined = marketsById[coin];
      if (!market && coin.includes(':')) {
        market = this.parent.marketsLowerCaseMap.get(coin.toLowerCase());
      }
      if (!market) continue;

      const positionMode = this.resolvePositionMode(assetPosition);
      const side = +assetPosition.position.szi > 0 ? PositionSide.Long : PositionSide.Short;
      const realizedPnl = this.getRealizedPnlForPosition(market.symbol, side);
      incomingPositions.push({
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

    // A dex-scoped empty snapshot is authoritative for closures on that DEX.
    // Preserve core positions and positions from every other HIP-3 DEX.
    const currentPositions = this.parent.store.positions;
    const fakes = this.parent.getFakePositions();
    const merged = new Map<string, Position>();
    for (let i = 0; i < fakes.length; i++) merged.set(fakes[i].id, fakes[i]);
    for (let i = 0; i < currentPositions.length; i++) {
      const position = currentPositions[i];
      const positionDex = this.parent.store.marketsBySymbol[position.symbol]?.info?.hip3Dex?.toLowerCase();
      if (!positionDex || positionDex !== messageDex) merged.set(position.id, position);
    }
    for (let i = 0; i < incomingPositions.length; i++) {
      merged.set(incomingPositions[i].id, incomingPositions[i]);
    }
    this.parent.store.addOrUpdatePositionsFull(Array.from(merged.values()));
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
    // Spot fills arrive on this same non-dex-scoped `userFills` channel with a
    // finite `startPosition`, so every guard above passes for them now that
    // spot markets resolve in `marketsById`. Synthesizing a Position from one
    // would invent a phantom long: it pollutes the positions table, feeds
    // `syncBalanceFromLivePositions` (corrupting `balance.upnl` / `free`, i.e.
    // order sizing) and `DisplayedMarginPostprocessor` exposure, and persists
    // until the next authoritative clearinghouse snapshot because the
    // streaming path merges incrementally. Spot exposure is a coin balance,
    // never a Position — same invariant as `generateFakePositionsFromMarkets`.
    if (market.isSpot) {
      // Spot never becomes a Position, but the fill DID change the wallet, and
      // nothing else on this channel would refresh it — spot holdings would
      // sit stale until reconnect/reload. The refresh is cooldown-throttled
      // and collapses in-flight calls, so firing per fill is safe.
      this.parent.requestUnifiedSpotCollateralRefresh();
      return;
    }

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
    if (!this.parent.shouldTrackSpotHoldings()) return;
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
      // Every order we place carries a cloid, and the frame echoes it.
      const cloid = readHyperliquidCloid(orderData.cloid);

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
        this.parent.suppressCancelledOrder(orderId);
        this.parent.store.removeOrder({ id: orderId });
        this.parent.forgetLimitOrderTimeInForce(orderId);
        if (cloid) this.parent.forgetPlacementRecord(cloid);
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
        const side = orderData.side === 'B' ? OrderSide.Buy : OrderSide.Sell;

        // PRIMARY: an order we placed is identified by the cloid it went out
        // with — exact type, trigger price, reduce-only and tif, recorded
        // before the REST call. Everything below is the FALLBACK for frames
        // with no cloid or no record: orders placed from another client, or a
        // context the record never reached (the hosted exchange-server).
        const record = cloid ? this.parent.getPlacementRecord(cloid) : undefined;

        // For HIP-3 orders, check pending cache first (just placed, shared across instances)
        const pendingOrder = !record && isHip3 ? this.parent.getPendingHip3Order(orderId) : null;

        if (record && cloid) {
          inferredType = record.type;
          inferredPrice = record.price;
          inferenceSource = 'cloid-record';
          // Its pre-placement entry is no longer needed; left behind, the FIFO
          // fallback could hand it to a different, cloid-less frame.
          this.parent.forgetPrePlacement(orderData.coin, side, cloid);
        } else if (pendingOrder) {
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
        } else if (
          !isHip3 &&
          Boolean(orderData.reduceOnly) &&
          !orderData.orderType &&
          !orderData.triggerPx &&
          !orderData.isTrigger
        ) {
          // FALLBACK since every order we place carries a cloid record (handled
          // above): this deferral now only meets frames with no cloid or no
          // record — reduce-only orders from another client, or the hosted
          // exchange-server, which never sees the browser's records.
          //
          // Core-perp reduce-only order with no trigger fields: Hyperliquid's
          // first WS `orderUpdates` message for a freshly-placed TP/SL
          // arrives with `orderType`, `triggerPx`, and `isTrigger` all
          // `undefined` — only `limitPx` and `reduceOnly` are populated. The
          // trigger fields only materialize on the REST `frontendOpenOrders`
          // snapshot (~500–700 ms later) or on subsequent WS frames.
          //
          // This used to require `origSz === 0` (positional TP/SL only). A
          // SIZED TP/SL — the legs attached to a limit entry via
          // `normalTpsl`, or a partial stop — fell through to the generic
          // inference below and was inserted as a Limit at `limitPx`. For an
          // attached leg that `limitPx` is the ENTRY price, so the chart drew
          // the stop sitting on the break-even line until REST corrected it;
          // for a standalone sized stop it is mark ±5%. Widened 2026-10-11 to
          // every reduce-only frame without trigger fields. The cost is that
          // a plain reduce-only limit now lands in the store on the next REST
          // refresh (~0.5–1.5 s) instead of its first WS frame; one placed
          // from this app is covered by the web's optimistic ghost meanwhile,
          // and one placed elsewhere just appears slightly later.
          //
          // Feeding that first WS frame through inferHyperliquidOrderTypeAndPrice()
          // misclassifies it as `type: Limit` at `limitPx`, which:
          //   (1) pollutes the store with the wrong price,
          //   (2) can be amplified by formatOrderPricesInBounds snapping the
          //       "limit" up to bestBid / down to bestAsk, and
          //   (3) prevents the chart's pending-no-id trigger claim from
          //       firing (that path requires isProtectiveStopOrder), so the
          //       chart spawns a phantom line alongside the parked ghost
          //       line, then does a DESTRUCTIVE move when REST finally
          //       corrects the type — producing the "SL teleports between
          //       3–4 positions" flicker.
          //
          // Preserve type/price if we already have this oid in the store
          // (subsequent WS frame after REST has arrived); otherwise defer:
          // skip the insert and schedule an accelerated REST refresh. The
          // chart-side Updating ghost keeps the parked line at the correct
          // price meanwhile, and when REST arrives with the real
          // `Stop Market` / `Take Profit Market` fields the pending-no-id
          // trigger claim fires as a RENAME (no phantom, no destructive move).
          const existingOrder = existingOrdersById[orderId];
          if (existingOrder) {
            inferredType = existingOrder.type;
            inferredPrice = existingOrder.price;
            inferenceSource =
              existingOrder.status === OrderStatus.Creating || existingOrder.status === OrderStatus.Updating
                ? 'optimistic-preserved'
                : 'store-preserved';
          } else {
            this.parent.log(
              `[orderUpdates] Deferring reduce-only insert (core-perp WS lacks orderType/triggerPx/isTrigger): ` +
                `oid=${orderId} coin=${orderData.coin} limitPx=${orderData.limitPx}. Scheduling REST refresh.`,
              LogSeverity.Debug,
            );
            this.parent.scheduleOrderRefresh();
            continue;
          }
        } else {
          // FALLBACK since the cloid record (above) identifies our own stop
          // entries exactly; this only answers for a frame that arrived without
          // its cloid.
          //
          // Core-perp STOP ENTRY: a non-reduce-only trigger placed from this
          // app is recorded in the pre-placement cache; its first frame has
          // no trigger fields and would read as a Limit at the slippage cap.
          // The strict matcher only answers for a trigger-type entry with the
          // same size, so a plain limit cannot claim it. Frames for orders
          // already in the store keep what REST/placement established.
          const existingOrder = !isHip3 ? existingOrdersById[orderId] : undefined;
          const stopEntry =
            !isHip3 && !existingOrder && !orderData.orderType && !orderData.triggerPx && !orderData.isTrigger
              ? this.parent.matchPrePlacementTrigger(orderData.coin, side, +orderData.origSz)
              : undefined;
          if (stopEntry) {
            inferredType = stopEntry.type;
            inferredPrice = stopEntry.price;
            inferenceSource = 'pre-placement-trigger';
          } else if (existingOrder && !orderData.orderType && !orderData.triggerPx && !orderData.isTrigger) {
            inferredType = existingOrder.type;
            inferredPrice = existingOrder.price;
            inferenceSource = 'store-preserved';
          } else {
            const inferred = inferHyperliquidOrderTypeAndPrice(orderData);
            inferredType = inferred.type;
            inferredPrice = inferred.price;
            inferenceSource = inferred.source;
          }
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

        // Determine timeInForce: use WS data if present, otherwise preserve.
        // `orderUpdates` omits tif for EVERY market, not just HIP-3 ones, so
        // without this fallback each update wipes the value REST/placement set.
        let inferredTimeInForce = record ? record.timeInForce : parseHyperliquidOrderTimeInForce(orderData.tif);
        if (!inferredTimeInForce && !record) {
          // Store first (O(1) via the pre-built map), then the placement-time
          // record — core perps are never REST-reconciled after startup, so for
          // them the placement record is the only source.
          inferredTimeInForce =
            existingOrdersById[orderId]?.timeInForce ?? this.parent.getPlacedLimitOrderTimeInForce(orderId);
        }

        // In hosted mode the order was placed on the browser mirror, so this
        // instance never saw the placement and REST is its only tif source.
        // Debounced + rate-limited, so a burst of orders costs one fetch.
        // A recorded order's tif is what we sent; REST has nothing to add.
        if (!inferredTimeInForce && !record && inferredType === OrderType.Limit) {
          this.parent.scheduleOrderRefresh();
        }

        const order: Order = {
          id: orderId,
          status: orderStatus,
          symbol: market.symbol,
          type: inferredType,
          side,
          price: inferredPrice,
          amount: +orderData.origSz,
          filled: +orderData.origSz - +orderData.sz,
          remaining: +orderData.sz,
          // Frames omit `reduceOnly` for orders that are not; the record knows.
          reduceOnly: record ? record.reduceOnly : Boolean(orderData.reduceOnly),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          timeInForce: inferredTimeInForce,
          ...(cloid ? { clientOrderId: cloid } : {}),
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
