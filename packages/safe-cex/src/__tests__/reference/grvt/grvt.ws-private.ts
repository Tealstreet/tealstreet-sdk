import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { resolveTransportRouting } from '../../../transport/registry';
import {
  LogSeverity,
  MarginType,
  Order,
  OrderSide,
  OrderStatus,
  OrderType,
  Position,
  PositionSide,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { GrvtExchange } from './grvt.exchange';
import { getGrvtOrderTimeInForce, getGrvtTriggerMetadata } from './grvt.order-semantics';

const parsePositiveFiniteDecimal = (value: unknown): number | undefined => {
  const amount =
    typeof value === 'number'
      ? value
      : typeof value === 'string' && /^(?:\d+(?:\.\d+)?|\.\d+)$/.test(value)
        ? Number(value)
        : Number.NaN;

  return Number.isFinite(amount) && amount > 0 ? amount : undefined;
};

/**
 * GRVT Private WebSocket - Account and trading streams
 *
 * Connects through the routing-table-selected Tealstreet WebSocket proxy.
 *
 * Automatically subscribes to:
 * - v1.order: Real-time order updates → updates store.orders
 * - v1.position: Position updates → updates store.positions
 * - v1.fill: Trade execution notifications → emits 'fill' events
 *
 * Requires authentication via session cookie and account ID sent to proxy
 */
export class GrvtPrivateWebSocket extends BaseWebSocket<GrvtExchange> {
  private reportedMissingProxyHost = false;
  // GRVT's private stream is purely event-driven (only fires on
  // order/position/fill changes) and goes minutes-long idle when the user
  // isn't trading. Under the default pong-miss watchdog, even reliable pongs
  // can race with the 90s reconnect threshold. Any inbound traffic —
  // including the server's own pings — counts as proof of liveness.
  protected override treatAnyMessageAsPong = true;

  private subscriptions: Map<number, any> = new Map();
  private requestId = 1;
  private authenticated = false;
  /**
   * A terminal authentication failure deliberately survives close events. The
   * parent exchange must be recreated (or explicitly force-opened elsewhere)
   * rather than allowing the base socket's native-close retry path to revive a
   * stream whose credentials have just been rejected.
   */
  private authenticationFailed = false;
  private sessionRefreshInFlight = false;
  private sessionRefreshAttempts = 0;
  private static readonly MAX_SESSION_REFRESH_ATTEMPTS = 1;
  /**
   * Proxy auth only proves that the proxy accepted its first frame. A session
   * becomes healthy only after a private feed has remained usable for this
   * bounded period; a quick auth_success -> 1000 sequence must retain the
   * original refresh budget.
   */
  private privateStreamHealthyAt?: number;
  private static readonly HEALTHY_SESSION_RESET_MS = 5 * 60_000;

  // Relaxed idle thresholds for event-driven private stream.
  // This WS only sends messages on order/position/fill changes, so long
  // silence is normal when the user isn't actively trading on GRVT.
  // The default 6s/20s thresholds cause a reconnect cycle every ~20-30s
  // which flickers position data in the UI.
  private static readonly IDLE_PING_THRESHOLD_MS = 30_000;
  private static readonly IDLE_RECONNECT_THRESHOLD_MS = 90_000;

  // Throttle state for positions
  private lastPositionFetchTime = 0;
  private positionFetchThrottleMs = 500;
  private positionFetchScheduled = false;
  private hasPendingPositionEvents = false;

  // Throttle state for orders
  private ordersThrottleTimeout?: ReturnType<typeof setTimeout>;

  // Fill deduplication: tracks trade IDs we've already emitted to prevent duplicate toasters
  private processedFillIds = new Set<string>();
  private static readonly MAX_PROCESSED_FILLS = 500;

  constructor(parent: GrvtExchange) {
    super(parent);
  }

  ping = () => {
    if (!this.isDisposed && !this.authenticationFailed) {
      this.pingAt = performance.now();
      this.send('ping');
    }
  };

  /**
   * Override base watchdog stale threshold so the exchange-level auditor
   * uses the same relaxed window as our checkLastReceived override.
   */
  getWatchdogStaleThresholdMs = (): number => GrvtPrivateWebSocket.IDLE_RECONNECT_THRESHOLD_MS;

  /**
   * Override base checkLastReceived with relaxed thresholds.
   * The private stream is event-driven (orders/positions/fills only),
   * so silence is expected when the user isn't actively trading on GRVT.
   */
  checkLastReceived = () => {
    const now = Date.now();
    const diff = now - this.lastReceived;
    const lastReceivedTime = new Date(this.lastReceived).toISOString();

    if (diff > GrvtPrivateWebSocket.IDLE_RECONNECT_THRESHOLD_MS) {
      this.parent.log(
        `[GRVT][PWS] No private stream messages for ${diff}ms (last at ${lastReceivedTime}); reconnecting.`,
        LogSeverity.Warning,
      );
      clearTimeout(this.lastReceivedTimer);
      this.lastReceived = 0;
      this.pingSent = false;
      this.reconnect();
    } else if (diff > GrvtPrivateWebSocket.IDLE_PING_THRESHOLD_MS && !this.pingSent) {
      this.parent.log(
        `[GRVT][PWS] No private stream messages for ${diff}ms (last at ${lastReceivedTime}); sending ping.`,
        LogSeverity.Info,
      );
      try {
        this.ping();
        this.pingSent = true;
      } catch (_e) {}
    } else if (diff <= GrvtPrivateWebSocket.IDLE_PING_THRESHOLD_MS) {
      this.pingSent = false;
    }
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      // Connect to the proxy WebSocket - auth happens after connection
      const host = resolveTransportRouting(this.parent.name, this.parent.options).wsPrivate.host;
      if (!host) {
        // Every reconnect lands here again; one error per instance is the signal.
        if (!this.reportedMissingProxyHost) {
          this.reportedMissingProxyHost = true;
          this.parent.emitAndLogError(
            'GRVT private stream needs a proxy transport (its session cookie cannot be set from a browser); the active transport resolves none.',
          );
        }
        return;
      }
      this.ws = this.createWebsocket(host);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
      this.ws.addEventListener('error', ((event: Event) => {
        this.parent.log(`WebSocket connection error: ${(event as any)?.message || 'Unknown error'}`, LogSeverity.Error);
      }) as any);
    }
  };

  onOpen = () => {
    if (!this.isDisposed && !this.authenticationFailed) {
      this.authenticated = false;
      this.privateStreamHealthyAt = undefined;
      this.parent.log('GRVT private WebSocket connected to proxy');

      // Send proxy authentication message (must be first message)
      this.authenticate();
    }
  };

  private authenticate = () => {
    // Send full cookie string with rm=true prefix for persistent session
    const sessionCookie = this.parent.apiExtras?.sessionCookie;
    const fullCookie = sessionCookie ? `rm=true; gravity=${sessionCookie}` : '';

    const authMessage = {
      type: 'auth',
      session_cookie: fullCookie,
      account_id: this.parent.apiExtras?.accountId,
      testnet: this.parent.options.testnet || false,
    };

    this.parent.log('Sending authentication to GRVT proxy');
    this.send(JSON.stringify(authMessage));
  };

  private resetRetryBudgetAfterHealthySession = () => {
    if (
      this.privateStreamHealthyAt !== undefined &&
      Date.now() - this.privateStreamHealthyAt >= GrvtPrivateWebSocket.HEALTHY_SESSION_RESET_MS
    ) {
      this.sessionRefreshAttempts = 0;
      // Reset only once per authenticated session; otherwise every later 1000
      // would look like a separate healthy window.
      this.privateStreamHealthyAt = undefined;
    }
  };

  private refreshSessionAndReconnect = (reason: 'proxy authentication rejected' | 'GRVT error 1000' | 'session refresh failed') => {
    if (this.sessionRefreshInFlight || this.isDisposed || this.authenticationFailed) {
      return;
    }

    this.resetRetryBudgetAfterHealthySession();

    if (this.sessionRefreshAttempts >= GrvtPrivateWebSocket.MAX_SESSION_REFRESH_ATTEMPTS) {
      this.failClosedForAuthentication(reason);
      return;
    }

    this.sessionRefreshInFlight = true;
    this.sessionRefreshAttempts += 1;
    this.authenticated = false;

    void (async () => {
      try {
        const refreshed = await this.parent.authenticate();
        if (!refreshed || !this.parent.apiExtras?.sessionCookie || !this.parent.apiExtras.accountId) {
          throw new Error('GRVT authentication did not return a complete session');
        }
        if (this.isDisposed) return;

        this.parent.log(`Refreshing GRVT private WebSocket session after ${reason}`, LogSeverity.Warning);
        await this.reconnect();
      } catch (_error) {
        this.failClosedForAuthentication('session refresh failed');
      } finally {
        this.sessionRefreshInFlight = false;
      }
    })();
  };

  private failClosedForAuthentication = (reason: 'proxy authentication rejected' | 'GRVT error 1000' | 'session refresh failed') => {
    this.authenticated = false;
    this.authenticationFailed = true;
    // Do not include proxy/GRVT payloads or caught errors here: those can
    // contain request context. The user-visible remediation is actionable and
    // safe to persist in diagnostics.
    this.parent.emitAndLogError(
      `GRVT private WebSocket authentication failed closed (${reason}). Check the API key and account authorization, then reconnect.`,
    );
    this.dispose();
  };

  private subscribe = () => {
    // Subscribe to all relevant streams for this sub-account
    const subAccountId = this.parent.grvtTradingAccountId;

    if (!subAccountId) {
      this.parent.log('No trading account ID provided - skipping private WS subscriptions', LogSeverity.Warning);
      return;
    }

    // Use just sub_account_id to subscribe to ALL instruments
    // Per GRVT docs: "To subscribe to all orders, specify an empty instrument (eg. 2345123)"
    const feed = [subAccountId];

    this.parent.log(`Subscribing to private streams for sub-account: ${subAccountId}`);

    // Subscribe to order updates
    this.subscribeToStream('v1.order', feed);

    // Subscribe to position updates
    this.subscribeToStream('v1.position', feed);

    // Subscribe to fill notifications
    this.subscribeToStream('v1.fill', feed);
  };

  /**
   * Subscribe to a private stream using legacy format (not JSON-RPC)
   * GRVT uses: { method, stream, feed } format
   */
  private subscribeToStream = (stream: string, feed: string[]) => {
    const id = this.requestId++;

    // Use legacy format as per GRVT SDK
    const subscription = {
      request_id: id,
      method: 'subscribe',
      stream,
      feed,
      is_full: false, // Use lite format for efficiency
    };

    this.subscriptions.set(id, subscription);
    this.send(JSON.stringify(subscription));
    return id;
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed && !this.authenticationFailed) {
      this.trackLastReceived();

      // Handle ping/pong messages
      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }
      if (data === 'ping') {
        this.send('pong');
        return;
      }

      try {
        const json = jsonParse(data);
        if (!json) return;

        // Handle proxy auth responses
        if (json.type === 'auth_success') {
          this.authenticated = true;
          this.parent.log('GRVT proxy WebSocket authenticated');
          this.subscribe();
          return;
        }

        if (json.type === 'error') {
          this.refreshSessionAndReconnect('proxy authentication rejected');
          return;
        }

        // Handle subscription confirmations
        if (json.result && json.id) {
          this.parent.log(`Private WS subscription confirmed (requestId=${json.id})`);
          return;
        }

        // Handle error responses
        if (json.error) {
          const errorCode = json.error.code ?? json.error.c;
          if (errorCode == 1000) {
            this.refreshSessionAndReconnect('GRVT error 1000');
            return;
          }
          this.parent.emitAndLogError(
            `Private WS error (code=${typeof errorCode === 'number' ? errorCode : 'unknown'})`,
          );
          return;
        }

        // Handle GRVT-specific error format: { code, message, status }
        if (json.code && json.status) {
          if (json.code == 1000) {
            this.refreshSessionAndReconnect('GRVT error 1000');
            return;
          }
          // Skip malformed syntax errors (1003) - these are noise
          if (json.code === 1003) {
            return;
          }
          // IP whitelisting error
          if (json.code === 1008) {
            this.parent.emitAndLogError(
              'GRVT IP is not whitelisted. Please re-add your API key with correct IP whitelisting (18.176.98.246).',
            );
          } else {
            this.parent.emitAndLogError(`GRVT error (code=${json.code})`);
          }
          return;
        }

        // Handle stream updates (support both full and lite formats)
        // Full format: { stream: 'v1.order', feed: {...} }
        // Lite format: { s: 'v1.order', f: {...} }
        const stream = json.stream || json.s;
        if (stream) {
          // This is the explicit healthy-session point. It must survive for
          // HEALTHY_SESSION_RESET_MS before a later auth failure earns a new
          // single refresh attempt.
          if (this.authenticated && this.privateStreamHealthyAt === undefined) {
            this.privateStreamHealthyAt = Date.now();
          }
          switch (stream) {
            case 'v1.order':
            case 'v1.state':
            case 'v1.cancel':
              this.handleOrderUpdate(json);
              break;
            case 'v1.position':
              this.handlePositionUpdate(json);
              break;
            case 'v1.fill':
              this.handleFillUpdate(json);
              break;
          }
        }
      } catch (_error) {
        this.parent.emitAndLogError('Error parsing private WS message');
      }
    }
  };

  private handleOrderUpdate = (json: any) => {
    try {
      // Support both full format (feed) and lite format (f)
      const feed = json.feed || json.f;
      if (!feed) {
        this.parent.log('Order update: no feed data in message', LogSeverity.Debug);
        return;
      }

      // Parse order from GRVT format
      const order = this.parseOrder(feed);
      if (!order) {
        this.parent.log('Order update: parseOrder returned null', LogSeverity.Debug);
        return;
      }

      this.parent.log(
        `Order update: ${order.id} status=${order.status} symbol=${order.symbol} side=${order.side} amount=${order.amount}`,
        LogSeverity.Info,
      );

      // Handle orders based on their status (similar to OKX/BloFin pattern)
      if (order.status === OrderStatus.Closed) {
        // Order is FILLED - update status and remove from store.
        // Fill notifications are emitted exclusively by handleFillUpdate (v1.fill stream)
        // to avoid duplicate toasters (GRVT sends both v1.order and v1.fill for the same trade).
        this.parent.store.updateOrderStatus({ id: order.id }, OrderStatus.Closed);
        this.parent.store.removeOrder({ id: order.id });

        this.throttledFetchOrders();
      } else if (order.status === OrderStatus.Canceled) {
        // Order is CANCELLED/REJECTED - update status and remove from store
        this.parent.store.updateOrderStatus({ id: order.id }, OrderStatus.Canceled);
        this.parent.store.removeOrder({ id: order.id });

        // Throttled fetch to sync state
        this.throttledFetchOrders();
      } else {
        // Order is OPEN/PENDING - add or update in store
        this.parent.store.addOrUpdateOrder(order);

        // Throttled fetch to ensure consistency with REST API
        this.throttledFetchOrders();
      }

      this.recordMessageHandlerStats('orders');
    } catch (error) {
      this.parent.emitAndLogError(`Error handling order update: ${error}`);
    }
  };

  private handlePositionUpdate = (json: any) => {
    try {
      // Support both full format (feed) and lite format (f)
      const feed = json.feed || json.f;
      if (!feed) return;

      const instrument = feed.instrument || feed.i;
      const sizeRaw = feed.size ?? feed.s;
      const size = typeof sizeRaw === 'number' ? sizeRaw : parseFloat(sizeRaw || 'NaN');

      // Explicit zero-size update means the position was closed.
      // Clear existing non-zero position state for this symbol and reconcile with REST.
      if (instrument && Number.isFinite(size) && size === 0) {
        const market =
          this.parent.store.marketsById[instrument] ||
          this.parent.store.marketsById[instrument.toUpperCase()] ||
          this.parent.store.markets.find((m) => m.id.toUpperCase() === instrument.toUpperCase());

        if (market) {
          const openPositionsForSymbol = this.parent.store.positions.filter(
            (p) => p.symbol === market.symbol && Math.abs(p.contracts) > 0,
          );

          openPositionsForSymbol.forEach((position) => {
            this.parent.store.updatePosition(position, {
              contracts: 0,
              notional: 0,
              unrealizedPnl: 0,
              liquidationPrice: 0,
              lastUpdate: virtualClock.getCurrentTimeMs(),
            });
          });
        }

        this.parent.syncBalanceUpnlFromPositions();

        this.hasPendingPositionEvents = true;
        this.throttledFetchPositions();
        this.recordMessageHandlerStats('positions');
        return;
      }

      // Parse position from GRVT format
      const position = this.parsePosition(feed);
      if (!position) {
        this.parent.log('Position update: parsePosition returned null', LogSeverity.Debug);
        // Still trigger REST reconciliation for partial/unexpected payloads.
        this.hasPendingPositionEvents = true;
        this.throttledFetchPositions();
        return;
      }

      this.parent.log(
        `Position update: ${position.symbol} ${position.side} ${position.contracts} contracts`,
        LogSeverity.Info,
      );

      // Find existing position in store
      const existingPosition = this.parent.store.positions.find((p) => p.symbol === position.symbol);

      if (existingPosition) {
        // Update existing position
        this.parent.log(`Updating existing position for ${position.symbol}`, LogSeverity.Debug);
        this.parent.store.updatePosition(existingPosition, position);
      } else {
        // Add new position
        this.parent.log(`Adding new position for ${position.symbol}`, LogSeverity.Debug);
        this.parent.store.update({
          positions: [...this.parent.store.positions, position],
        });
      }

      this.parent.syncBalanceUpnlFromPositions();

      // Throttled fetch to ensure consistency with REST API
      this.hasPendingPositionEvents = true;
      this.throttledFetchPositions();

      this.recordMessageHandlerStats('positions');
    } catch (error) {
      this.parent.emitAndLogError(`Error handling position update: ${error}`);
    }
  };

  private handleFillUpdate = (json: any) => {
    try {
      // Support both full format (feed) and lite format (f)
      const feed = json.feed || json.f;
      if (!feed) return;

      // GRVT fill format (full/lite):
      //   instrument/i, is_buyer/ib (boolean), size/s, price/p, trade_id/ti, order_id/oi
      const instrument = feed.instrument || feed.i;
      const isBuyer = feed.is_buyer !== undefined ? feed.is_buyer : feed.ib;
      const size = feed.size || feed.s;
      const price = feed.price || feed.p;
      const tradeId = feed.trade_id || feed.ti;
      // Fill events drive chaser remaining size. Accepting a partial parse
      // (or a negative/non-finite amount) here can enlarge or poison the
      // next cancel-replace order, so match the strict REST reconciliation
      // contract before deduplication or emission.
      const amount = parsePositiveFiniteDecimal(size);
      if (amount === undefined) {
        this.parent.log('Ignoring GRVT fill with invalid size', LogSeverity.Warning);
        return;
      }

      // Deduplicate fills to prevent duplicate toasters
      const fillKey = tradeId || `${instrument}-${price}-${size}-${Date.now()}`;
      if (this.processedFillIds.has(fillKey)) {
        return;
      }
      this.processedFillIds.add(fillKey);
      if (this.processedFillIds.size > GrvtPrivateWebSocket.MAX_PROCESSED_FILLS) {
        const first = this.processedFillIds.values().next().value!;
        this.processedFillIds.delete(first);
      }

      // Use O(1) hashmap lookup first, fall back to case-insensitive find if needed
      const market =
        this.parent.store.marketsById[instrument] ||
        this.parent.store.marketsById[instrument?.toUpperCase()] ||
        this.parent.store.markets.find((m) => m.id.toUpperCase() === instrument?.toUpperCase());
      if (!market) return;

      // Use nonce (client_order_id) as canonical order ID — same as parseOrder/placeOrder
      const fillClientOrderId = feed.client_order_id || feed.co;
      const fillOrderId = fillClientOrderId ? String(fillClientOrderId) : undefined;

      this.parent.log(
        `Fill event: tradeId=${tradeId} orderId=${fillOrderId} symbol=${market.symbol} size=${size} price=${price} feedKeys=${Object.keys(feed).join(',')}`,
        LogSeverity.Info,
      );

      this.parent.emitter.emit('fill', {
        id: tradeId,
        ...(fillOrderId ? { orderId: fillOrderId } : {}),
        side: isBuyer ? OrderSide.Buy : OrderSide.Sell,
        symbol: market.symbol,
        price: parseFloat(price),
        amount,
      });

      this.recordMessageHandlerStats('fills');
    } catch (_error) {
      this.parent.emitAndLogError('Error handling fill update');
    }
  };

  /**
   * Parse GRVT order format to safe-cex Order format
   */
  private parseOrder = (feed: any): Order | null => {
    try {
      // Get the first leg (support both full and lite formats)
      const firstLeg = feed.legs?.[0] || feed.l?.[0];
      const instrumentId = firstLeg?.instrument || firstLeg?.i;

      if (!instrumentId) {
        this.parent.log(
          `parseOrder: missing instrument ID (event=order, legCount=${Array.isArray(feed.legs || feed.l) ? (feed.legs || feed.l).length : 0})`,
          LogSeverity.Warning,
        );
        return null;
      }

      // Use O(1) hashmap lookup first, fall back to case-insensitive find if needed
      const market =
        this.parent.store.marketsById[instrumentId] ||
        this.parent.store.marketsById[instrumentId.toUpperCase()] ||
        this.parent.store.markets.find((m) => m.id.toUpperCase() === instrumentId.toUpperCase());

      if (!market) {
        this.parent.log(
          `parseOrder: market not found (event=order, marketCount=${this.parent.store.markets.length})`,
          LogSeverity.Warning,
        );
        return null;
      }

      // Handle both full and lite formats
      const rawOrderId = feed.order_id || feed.oi;
      const leg = feed.legs?.[0] || feed.l?.[0];
      const state = feed.state || feed.s1;
      const metadata = feed.metadata || feed.m;

      // GRVT's oi is "0x00" until the server assigns a real ID (asynchronous).
      // Use the nonce (metadata.co / client_order_id) as the canonical order ID —
      // it's consistent with what placeOrder returns and what REST fetches include.
      const clientOrderId = metadata?.client_order_id || metadata?.co;
      let orderId = clientOrderId ? String(clientOrderId) : rawOrderId;

      // If we got a real GRVT order_id, save the nonce→realId mapping for cancelOrder
      const isRealOrderId = rawOrderId && rawOrderId !== '0x00' && rawOrderId !== '0x0';
      if (clientOrderId) {
        const exchange = this.parent as GrvtExchange;
        exchange.recordNonce(
          String(clientOrderId),
          isRealOrderId ? rawOrderId : undefined,
          exchange.getOrderCreationTimeMs(metadata),
        );
      }

      if (!orderId || orderId === '0x00' || orderId === '0x0') {
        this.parent.log(
          `parseOrder: no usable order ID (event=order, hasRawOrderId=${Boolean(rawOrderId)}, hasClientOrderId=${Boolean(clientOrderId)})`,
          LogSeverity.Warning,
        );
      }

      // DEBUG: Log if state is missing
      if (!state) {
        this.parent.log(
          `parseOrder: missing state (event=order, hasFullState=${Boolean(feed.state)}, hasLiteState=${Boolean(feed.s1)})`,
          LogSeverity.Warning,
        );
      }

      if (!orderId || !leg || !state) {
        return null;
      }

      // Parse order status
      let status: OrderStatus;
      const grvtStatus = state.status || state.s;
      switch (grvtStatus) {
        case 'PENDING':
          status = OrderStatus.Open;
          break;
        case 'OPEN':
          status = OrderStatus.Open;
          break;
        case 'FILLED':
          status = OrderStatus.Closed;
          break;
        case 'REJECTED':
          status = OrderStatus.Canceled;
          break;
        case 'CANCELLED':
          status = OrderStatus.Canceled;
          break;
        default:
          status = OrderStatus.Open;
      }

      // Parse side
      const isBuying = leg.is_buying_asset !== undefined ? leg.is_buying_asset : leg.ib;
      const side = isBuying ? OrderSide.Buy : OrderSide.Sell;

      const trigger = getGrvtTriggerMetadata(feed);
      const isMarket = feed.is_market !== undefined ? feed.is_market : feed.im;
      const type =
        trigger?.type === 'STOP_LOSS'
          ? OrderType.StopLoss
          : trigger?.type === 'TAKE_PROFIT'
            ? OrderType.TakeProfit
            : isMarket
              ? OrderType.Market
              : OrderType.Limit;

      // Parse amounts
      const size = parseFloat(leg.size || leg.s || '0');
      const tradedSize = parseFloat(state.traded_size?.[0] || state.ts?.[0] || '0');
      const filled = tradedSize;
      const remaining = Math.max(0, size - filled);

      // Parse price
      const limitPrice = parseFloat(leg.limit_price || leg.lp || '0');
      const price = trigger?.triggerPrice ?? limitPrice;

      // Parse timestamp
      const updateTime = state.update_time || state.ut || metadata?.create_time || metadata?.ct;
      const lastUpdate = updateTime ? Math.floor(parseInt(updateTime) / 1_000_000) : virtualClock.getCurrentTimeMs();

      return {
        id: orderId,
        ...(clientOrderId ? { clientOrderId: String(clientOrderId) } : {}),
        status,
        symbol: market.symbol,
        type,
        side,
        price,
        amount: size,
        filled,
        remaining,
        lastUpdate,
        reduceOnly: feed.reduce_only || feed.ro || false,
        timeInForce: getGrvtOrderTimeInForce(feed),
        ...(trigger?.close !== undefined ? { close: trigger.close } : {}),
        ...(type === OrderType.StopLoss && trigger?.triggerType ? { slTriggerType: trigger.triggerType } : {}),
        ...(type === OrderType.TakeProfit && trigger?.triggerType ? { tpTriggerType: trigger.triggerType } : {}),
        info: feed,
      };
    } catch (error) {
      this.parent.emitAndLogError(`Error parsing order: ${error}`);
      return null;
    }
  };

  /**
   * Parse GRVT position format to safe-cex Position format
   */
  private parsePosition = (feed: any): Position | null => {
    try {
      // Handle both full and lite formats
      const instrument = feed.instrument || feed.i;
      if (!instrument) {
        this.parent.log('Position update: missing instrument in feed', LogSeverity.Debug);
        return null;
      }

      const sizeRaw = feed.size ?? feed.s;
      if (sizeRaw === undefined || sizeRaw === null) {
        this.parent.log(`Position update: missing size for instrument ${instrument}`, LogSeverity.Debug);
        return null;
      }

      const size = parseFloat(sizeRaw);
      if (!Number.isFinite(size)) {
        this.parent.log(`Position update: invalid size for instrument ${instrument}`, LogSeverity.Debug);
        return null;
      }

      // Use O(1) hashmap lookup first, fall back to case-insensitive find if needed
      const market =
        this.parent.store.marketsById[instrument] ||
        this.parent.store.marketsById[instrument.toUpperCase()] ||
        this.parent.store.markets.find((m) => m.id.toUpperCase() === instrument.toUpperCase());
      if (!market) {
        this.parent.log(`Position update: market not found for instrument ${instrument}`, LogSeverity.Warning);
        return null;
      }

      // Determine position side
      let side: PositionSide;
      if (size > 0) {
        side = PositionSide.Long;
      } else if (size < 0) {
        side = PositionSide.Short;
      } else {
        // No position
        return null;
      }

      const contracts = Math.abs(size);
      const notional = Math.abs(parseFloat(feed.notional || feed.n || '0'));
      const entryPrice = parseFloat(feed.entry_price || feed.ep || '0');
      const liquidationPrice = parseFloat(feed.liquidation_price || feed.lp || '0');
      const unrealizedPnl = parseFloat(feed.unrealized_pnl || feed.up || '0');
      const leverage = parseFloat(feed.leverage || feed.l || '1');

      return {
        id: createPositionId(market.symbol),
        symbol: market.symbol,
        side,
        notional,
        leverage,
        entryPrice,
        unrealizedPnl,
        contracts,
        liquidationPrice,
        marginType: (feed.mt ?? feed.margin_type) === 'CROSS' ? MarginType.Cross : MarginType.Isolated,
      };
    } catch (error) {
      this.parent.emitAndLogError(`Error parsing position: ${error}`);
      return null;
    }
  };

  /**
   * Throttled fetch orders - debounces with 1000ms timeout
   * Called on order update events to refresh orders from REST API
   */
  private throttledFetchOrders = () => {
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
    }
    this.ordersThrottleTimeout = setTimeout(this.fetchThrottledOrders, 1000);
  };

  private fetchThrottledOrders = async () => {
    this.ordersThrottleTimeout = undefined;
    if (!this.isDisposed) {
      try {
        const ordersBefore = this.parent.store.orders.length;
        const orders = await this.parent.fetchOrders();
        this.parent.log(
          `Throttled fetch: received ${orders.length} open orders from API (was ${ordersBefore} in store)`,
          LogSeverity.Info,
        );

        // SAFEGUARD: Don't wipe orders if API returns empty but we have orders in store
        // This prevents losing orders due to temporary API issues or parsing problems
        if (ordersBefore > 0 && orders.length === 0) {
          this.parent.log(
            `WARNING: Skipping store update - API returned 0 orders but store has ${ordersBefore}. Keeping existing orders.`,
            LogSeverity.Warning,
          );
          return;
        }

        this.parent.store.update({
          orders,
          loaded: {
            ...this.parent.store.loaded,
            orders: true,
          },
        });
      } catch (error) {
        this.parent.log(`Throttled fetch error: ${error}`, LogSeverity.Warning);
        // Silent failure, will retry on next event
      }
    }
  };

  /**
   * Throttled fetch positions - throttles with 500ms interval
   * Called on position update events to refresh positions from REST API
   */
  private throttledFetchPositions = () => {
    const now = Date.now();

    if (this.isDisposed) {
      return;
    }

    // If a fetch is already scheduled, mark as having pending events
    // and let the scheduled fetch handle it
    if (this.positionFetchScheduled) {
      return;
    }

    const timeSinceLastFetch = now - this.lastPositionFetchTime;

    if (timeSinceLastFetch >= this.positionFetchThrottleMs) {
      // Execute immediately if enough time has passed
      this.executePositionFetch();
    } else {
      // Schedule for later if we need to throttle
      this.positionFetchScheduled = true;
      setTimeout(() => {
        this.positionFetchScheduled = false;
        if (!this.isDisposed) {
          // Always execute if we had pending events
          if (this.hasPendingPositionEvents) {
            this.executePositionFetch();
          }
        }
      }, this.positionFetchThrottleMs - timeSinceLastFetch);
    }
  };

  private executePositionFetch = async () => {
    try {
      this.lastPositionFetchTime = Date.now();
      this.hasPendingPositionEvents = false;
      const positions = await this.parent.fetchPositions();
      if (this.isDisposed) {
        return;
      }

      this.parent.store.update({
        positions,
        loaded: {
          ...this.parent.store.loaded,
          positions: true,
        },
      });
      this.parent.syncBalanceUpnlFromPositions(positions);
    } catch {
      // Silent failure, will retry on next event
    }
  };

  dispose = () => {
    this.subscriptions.clear();
    this.processedFillIds.clear();
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
      this.ordersThrottleTimeout = undefined;
    }
    this.privateStreamHealthyAt = undefined;
    super.dispose();
  };
}
