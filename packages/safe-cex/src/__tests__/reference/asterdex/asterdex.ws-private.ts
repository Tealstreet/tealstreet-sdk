import type { Store } from '../../../store/store.interface';
import type { AsterDEXExchange } from './asterdex.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { Balance, LogSeverity, OrderSide, OrderStatus, PositionSide } from '../../../types';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { ASTERDEX_ENDPOINTS } from '../../../exchanges/asterdex/asterdex.types';

type MessageHandlers = {
  [eventType: string]: Array<(data: any) => void>;
};

type AssetBalanceUpdate = {
  asset: string;
  total: number;
  free: number;
  used: number;
};

export class AsterDEXPrivateWebsocket extends BaseWebSocket<AsterDEXExchange> {
  // Aster's account-level fields (totalWalletBalance, availableBalance, and
  // totalUnrealizedProfit) are USDT-denominated, including multi-asset mode.
  private static readonly ACCOUNT_CURRENCY = 'USDT';
  private static readonly LISTEN_KEY_REFRESH_MS = 30 * 60 * 1000;
  private static readonly LISTEN_KEY_FAILURE_RETRY_MS = 30 * 1000;
  private static readonly LISTEN_KEY_WATCHDOG_INTERVAL_MS = 5 * 60 * 1000;
  private static readonly LISTEN_KEY_WATCHDOG_STALE_MS = 70 * 60 * 1000;
  private static readonly POSITION_REPAIR_DELAY_MS = 1000;
  private static readonly ORDER_RECONCILE_DEBOUNCE_MS = 10_000;

  private listenKey = '';
  private lastListenKeyKeepAliveAt = 0;
  private listenKeyConsecutiveFailures = 0;
  private listenKeyRefreshTimer?: NodeJS.Timeout;
  private listenKeyWatchdogTimer?: NodeJS.Timeout;
  private connectRetryTimer?: NodeJS.Timeout;
  private connectRetryCount = 0;
  private positionRepairTimer?: NodeJS.Timeout;
  private positionRepairInFlight = false;
  private positionRepairQueued = false;
  private balanceReconcileTimer?: NodeJS.Timeout;
  private balanceReconcileInFlight = false;
  private balanceReconcileQueued = false;
  private orderReconcileTimer?: NodeJS.Timeout;
  private lastAccountUpdateEventTime = 0;

  messageHandlers: MessageHandlers = {
    // AsterDEX uses the same event types as Binance Futures
    ORDER_TRADE_UPDATE: [(d: any) => this.handleOrderTradeUpdate(d)],
    ACCOUNT_UPDATE: [(d: any) => this.handleAccountPositionUpdate(d)],
    listenKeyExpired: [(d: any) => this.handleListenKeyExpired(d)],
    // Legacy/spot event types (kept for compatibility)
    executionReport: [(d: any) => this.handleExecutionReport(d)],
    outboundAccountPosition: [(d: any) => this.handleAccountUpdate(d)],
  };

  constructor(parent: AsterDEXExchange, _store: Store) {
    super(parent);

    // BaseWebSocket.onClose is an instance arrow-function field (not a
    // prototype method), so retain the base instance handler and wrap it.
    const baseOnClose = this.onClose;
    this.onClose = (event?: any): void => {
      this.clearListenKeyRefreshTimer();
      this.clearListenKeyWatchdogTimer();
      this.clearPositionRepairTimer();
      this.clearBalanceReconcileTimer();
      this.clearOrderReconcileTimer();

      baseOnClose(event);
    };
  }

  connectAndSubscribe = async () => {
    if (this.isDisposed) return;

    try {
      // Fetch listen key directly like Binance
      const listenKey = await this.fetchListenKey();
      if (this.isDisposed) return;

      this.clearConnectRetryTimer();

      // User Data Streams are accessed at /ws/<listenKey>.
      const streamUrl = this.parent.options.testnet
        ? ASTERDEX_ENDPOINTS.WS.PRIVATE.TESTNET
        : ASTERDEX_ENDPOINTS.WS.PRIVATE.PRODUCTION;
      const baseUrl = streamUrl.replace('/stream', '');
      const url = `${baseUrl}/ws/${listenKey}`;

      this.parent.log(`Connecting to AsterDEX private WebSocket: ${url}`, LogSeverity.Info);

      this.ws = this.createWebsocket(url);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    } catch (error: any) {
      this.parent.log(
        `Failed to initialize AsterDEX private WebSocket: ${error?.message || 'Unknown error'}`,
        LogSeverity.Warning,
      );
      this.scheduleConnectRetry();
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.connectRetryCount = 0;
      this.parent.log('AsterDEX private WebSocket connected', LogSeverity.Info);

      // Send initial ping to establish pong tracking
      this.ping();

      // No explicit subscription needed for user data stream
      // Events are automatically sent once connected with valid listen key

      if (ENABLE_RECONNECTING_WEBSOCKET) {
        // Fetch initial snapshot of private data
        this.parent.fetchAndSetPrivateSnapshot(10);
      }
    }
  };

  ping = (): void => {
    // AsterDEX (like Binance) supports application-level ping via LIST_SUBSCRIPTIONS
    if (!this.isDisposed && this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.pingAt = performance.now();
      this.ws.send(JSON.stringify({ id: 42, method: 'LIST_SUBSCRIPTIONS' }));
    }
  };

  onMessage = (event: MessageEvent): void => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(event.data);
        if (!json) return;

        // Handle pong response
        if (json.id === 42) {
          this.handlePongEvent();
          return;
        }

        // Handle different event types
        const eventType = json.e;
        if (eventType && this.messageHandlers[eventType]) {
          this.recordMessageHandlerStats(eventType);
          this.messageHandlers[eventType].forEach((handler) => handler(json));
        } else if (json.e) {
          this.parent.log(
            `Unhandled private event type: ${json.e}, data: ${JSON.stringify(json).substring(0, 200)}`,
            LogSeverity.Warning,
          );
        } else {
          // Log raw message if no event type found
          this.parent.log(
            `Private WS message with no event type: ${JSON.stringify(json).substring(0, 200)}`,
            LogSeverity.Debug,
          );
        }
      } catch (error: any) {
        this.parent.log(`Failed to process private message: ${error.message}`, LogSeverity.Error);
      }
    }
  };

  private handleExecutionReport(data: any): void {
    // Handle spot/margin orders
    this.scheduleOrderReconcile();
    const order = this.parent.store.orders.find((o) => o.id === data.i?.toString());

    if (order) {
      // Map AsterDEX order status to our OrderStatus enum
      const statusMap: Record<string, OrderStatus> = {
        NEW: OrderStatus.Open,
        PARTIALLY_FILLED: OrderStatus.Open, // Treat as open since we don't have PartiallyFilled
        FILLED: OrderStatus.Closed,
        CANCELED: OrderStatus.Canceled,
        REJECTED: OrderStatus.Canceled, // Treat rejected as canceled
        EXPIRED: OrderStatus.Canceled, // Treat expired as canceled
      };

      const status = statusMap[data.X] || OrderStatus.Open;

      this.parent.store.updateOrder(order, {
        status,
        filled: parseFloat(data.z || '0'),
        remaining: parseFloat(data.q || '0') - parseFloat(data.z || '0'),
      });

      // Emit order update event
      this.parent.emitter.emit('order', order.id, order);
    }
  }

  private handleOrderTradeUpdate(data: any): void {
    // Handle futures orders
    const orderData = data.o;
    if (!orderData) return;

    this.scheduleOrderReconcile();

    // Handle NEW orders - add to store
    if (orderData.X === 'NEW') {
      this.parent.store.addOrUpdateOrder({
        id: orderData.i?.toString(),
        status: OrderStatus.Open,
        symbol: orderData.s,
        type: this.parent['reverseMapOrderType'](orderData.o),
        side: orderData.S === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
        price: parseFloat(orderData.p || '0') || parseFloat(orderData.sp || '0'),
        amount: parseFloat(orderData.q || '0'),
        filled: parseFloat(orderData.z || '0'),
        remaining: parseFloat(orderData.q || '0') - parseFloat(orderData.z || '0'),
        // For closePosition TP/SL orders, cp=true while R may be false/omitted.
        // Treat both as reduce-only in local state.
        reduceOnly: Boolean(orderData.R || orderData.cp),
        lastUpdate: virtualClock.getCurrentTimeMs(),
        info: orderData,
      });

      this.parent.log(
        `Order added to store: ${orderData.i} ${orderData.s} ${orderData.o} ${orderData.S}`,
        LogSeverity.Info,
      );
      return;
    }

    // Handle FILLED/PARTIALLY_FILLED - emit fill event
    if (orderData.X === 'PARTIALLY_FILLED' || orderData.X === 'FILLED') {
      this.mirrorCumulativeOrderFill(orderData);
      // The fill contract is `id` + `amount`; `l`/`L` are this execution's size and price.
      // A non-trade frame on a filled order carries l=0: no execution, no event.
      const executedAmount = parseFloat(orderData.l || '0');
      const lastPrice = parseFloat(orderData.L || '0');
      if (executedAmount > 0) {
        this.parent.emitter.emit('fill', {
          id: orderData.i?.toString(),
          orderId: orderData.i?.toString(),
          symbol: orderData.s,
          side: orderData.S === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
          price: lastPrice > 0 ? lastPrice : parseFloat(orderData.ap || '0'),
          amount: executedAmount,
        });
      }
    }

    // Handle FILLED - update status to closed
    if (orderData.X === 'FILLED') {
      const order = this.parent.store.orders.find((o) => o.id === orderData.i?.toString());
      if (order) {
        this.parent.store.updateOrder(order, {
          status: OrderStatus.Closed,
          filled: parseFloat(orderData.z || '0'),
          remaining: 0,
          lastUpdate: this.getOrderUpdateTime(orderData),
        });
        this.parent.emitter.emit('order', order.id, order);
      }
      return;
    }

    // Handle CANCELED/REJECTED/EXPIRED - update status to canceled
    if (orderData.X === 'CANCELED' || orderData.X === 'REJECTED' || orderData.X === 'EXPIRED') {
      const order = this.parent.store.orders.find((o) => o.id === orderData.i?.toString());
      if (order) {
        this.parent.store.updateOrder(order, {
          status: OrderStatus.Canceled,
          lastUpdate: this.getOrderUpdateTime(orderData),
        });
        this.parent.emitter.emit('order', order.id, order);
      }
      return;
    }

    // Handle other order updates
    const order = this.parent.store.orders.find((o) => o.id === orderData.i?.toString());
    if (order) {
      const statusMap: Record<string, OrderStatus> = {
        NEW: OrderStatus.Open,
        PARTIALLY_FILLED: OrderStatus.Open,
        FILLED: OrderStatus.Closed,
        CANCELED: OrderStatus.Canceled,
        REJECTED: OrderStatus.Canceled,
        EXPIRED: OrderStatus.Canceled,
      };

      const status = statusMap[orderData.X] || OrderStatus.Open;

      this.parent.store.updateOrder(order, {
        status,
        filled: parseFloat(orderData.z || '0'),
        remaining: parseFloat(orderData.q || '0') - parseFloat(orderData.z || '0'),
        lastUpdate: this.getOrderUpdateTime(orderData),
      });

      this.parent.emitter.emit('order', order.id, order);
    }
  }

  private mirrorCumulativeOrderFill(orderData: any): void {
    const orderId = orderData.i?.toString();
    const filled = parseFloat(orderData.z || '0');
    const amount = parseFloat(orderData.q || '0');

    if (!orderId || !Number.isFinite(filled) || !Number.isFinite(amount)) {
      return;
    }

    this.parent.store.updateOrder(
      { id: orderId },
      {
        status: orderData.X === 'FILLED' ? OrderStatus.Closed : OrderStatus.Open,
        filled,
        remaining: Math.max(amount - filled, 0),
        lastUpdate: this.getOrderUpdateTime(orderData),
      },
    );
  }

  private getOrderUpdateTime(orderData: any): number {
    const eventTime = Number(orderData.T || orderData.E);
    return Number.isFinite(eventTime) && eventTime > 0 ? eventTime : virtualClock.getCurrentTimeMs();
  }

  private parseFiniteNumber(value: unknown): number | undefined {
    if (typeof value !== 'number' && typeof value !== 'string') return undefined;
    if (typeof value === 'string' && value.trim() === '') return undefined;
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : undefined;
  }

  private parseAssetBalanceUpdates(
    balances: unknown,
    fields: { total?: string; free: string; used?: string },
  ): AssetBalanceUpdate[] | undefined {
    if (!Array.isArray(balances)) return undefined;

    const updates: AssetBalanceUpdate[] = [];
    for (const balance of balances) {
      const asset = typeof balance?.a === 'string' ? balance.a.trim() : '';
      const free = this.parseFiniteNumber(balance?.[fields.free]);
      const used = fields.used ? this.parseFiniteNumber(balance?.[fields.used]) : undefined;
      const total = fields.total
        ? this.parseFiniteNumber(balance?.[fields.total])
        : free !== undefined && used !== undefined
          ? free + used
          : undefined;
      const derivedUsed = used ?? (total !== undefined && free !== undefined ? total - free : undefined);
      if (!asset || total === undefined || free === undefined || derivedUsed === undefined || derivedUsed < 0) return undefined;
      updates.push({ asset, total, free, used: derivedUsed });
    }
    return updates;
  }

  private getPositionSide(position: any, positionAmount: number): PositionSide {
    return position.ps === 'BOTH'
      ? positionAmount > 0
        ? PositionSide.Long
        : PositionSide.Short
      : position.ps === 'LONG'
        ? PositionSide.Long
        : PositionSide.Short;
  }

  private getAccountUpdateUpnlDeltas(positions: unknown): Map<string, number> | undefined {
    const deltas = new Map<string, number>();
    if (positions === undefined) return deltas;
    if (!Array.isArray(positions)) return undefined;

    for (const position of positions) {
      const symbol = typeof position?.s === 'string' ? position.s : '';
      const positionAmount = this.parseFiniteNumber(position?.pa);
      const unrealizedPnl = this.parseFiniteNumber(position?.up);
      if (!symbol || positionAmount === undefined || unrealizedPnl === undefined) return undefined;

      const positionSide = this.getPositionSide(position, positionAmount);
      const existing = this.parent.store.positions.find((p) => p.symbol === symbol && p.side === positionSide);
      const previousUpnl = existing?.unrealizedPnl ?? 0;
      if (!Number.isFinite(previousUpnl)) return undefined;

      // Account-level Aster values are USDT-denominated. Keep per-asset
      // details when a matching collateral bucket already exists, but never
      // infer an FX conversion for another asset from this delta alone.
      const asset =
        Object.keys(this.parent.store.balance.currencies || {}).find((currency) => symbol.endsWith(currency)) ||
        (symbol.endsWith(AsterDEXPrivateWebsocket.ACCOUNT_CURRENCY)
          ? AsterDEXPrivateWebsocket.ACCOUNT_CURRENCY
          : undefined);
      if (asset) deltas.set(asset, (deltas.get(asset) || 0) + unrealizedPnl - previousUpnl);
    }
    return deltas;
  }

  private applyBalanceUpdates(updates: AssetBalanceUpdate[], upnlDeltas: Map<string, number>): boolean {
    const current = this.parent.store.balance;
    const currencies: Record<string, Balance> = { ...(current.currencies || {}) };

    for (const update of updates) {
      const previous = currencies[update.asset];
      const upnl = (previous?.upnl || 0) + (upnlDeltas.get(update.asset) || 0);
      currencies[update.asset] = {
        ...previous,
        total: update.total,
        free: update.free,
        used: update.used,
        upnl,
      };
    }

    const collateralUpdate = updates.find((update) => update.asset === AsterDEXPrivateWebsocket.ACCOUNT_CURRENCY);
    const collateralUpnlDelta = upnlDeltas.get(AsterDEXPrivateWebsocket.ACCOUNT_CURRENCY) || 0;
    const previousCollateral = current.currencies?.[AsterDEXPrivateWebsocket.ACCOUNT_CURRENCY];
    const needsAuthoritativeReconcile =
      updates.some((update) => update.asset !== AsterDEXPrivateWebsocket.ACCOUNT_CURRENCY) ||
      Object.keys(currencies).some((asset) => asset !== AsterDEXPrivateWebsocket.ACCOUNT_CURRENCY);
    let accountFields: Pick<Balance, 'total' | 'free' | 'used'> = {
      total: current.total,
      free: current.free,
      used: current.used,
    };
    if (collateralUpdate && !needsAuthoritativeReconcile) {
      // A standalone USDT update has the same single-asset semantics as the
      // account root. Multi-asset totals use Aster conversion rates and must
      // instead come from its authoritative account endpoint.
      accountFields = previousCollateral
        ? {
            total: current.total + collateralUpdate.total - previousCollateral.total,
            free: current.free + collateralUpdate.free - previousCollateral.free,
            used: current.used + collateralUpdate.used - previousCollateral.used,
          }
        : {
            total: collateralUpdate.total,
            free: collateralUpdate.free,
            used: collateralUpdate.used,
          };
    }
    const nextBalance: Balance = {
      ...current,
      currencies,
      upnl: needsAuthoritativeReconcile ? current.upnl : current.upnl + collateralUpnlDelta,
      ...accountFields,
    };

    if (
      !Number.isFinite(nextBalance.total) ||
      !Number.isFinite(nextBalance.free) ||
      !Number.isFinite(nextBalance.used) ||
      !Number.isFinite(nextBalance.upnl)
    ) {
      return needsAuthoritativeReconcile;
    }
    this.parent.store.update({ balance: nextBalance });
    return needsAuthoritativeReconcile;
  }

  private handleAccountUpdate(data: any): void {
    if (data.B) {
      const balances = this.parseAssetBalanceUpdates(data.B, { free: 'f', used: 'l' });
      if (balances) this.applyBalanceUpdates(balances, new Map());
    }

    this.parent.emitter.emit('account', data);
  }

  private handleAccountPositionUpdate(data: any): void {
    const eventTime = data?.E;
    if (typeof eventTime !== 'number' || !Number.isSafeInteger(eventTime) || eventTime <= 0) {
      this.parent.log('Ignoring ACCOUNT_UPDATE without a valid numeric event time E', LogSeverity.Warning);
      return;
    }
    if (eventTime <= this.lastAccountUpdateEventTime) {
      this.parent.log('Ignoring stale ACCOUNT_UPDATE ordered before the current event time E', LogSeverity.Debug);
      return;
    }
    if (!data?.a || typeof data.a !== 'object') {
      this.parent.log('Ignoring malformed ACCOUNT_UPDATE payload', LogSeverity.Warning);
      return;
    }

    const upnlDeltas = this.getAccountUpdateUpnlDeltas(data.a?.P);
    const balances = data.a.B === undefined ? [] : this.parseAssetBalanceUpdates(data.a.B, { total: 'wb', free: 'cw' });
    if (!upnlDeltas || !balances) {
      this.parent.log('Ignoring malformed ACCOUNT_UPDATE balance or position values', LogSeverity.Warning);
      return;
    }

    this.lastAccountUpdateEventTime = eventTime;
    const balanceStateVersion = this.parent.markBalanceStateUpdatedByWebsocket();

    // Handle ACCOUNT_UPDATE event for futures positions
    if (data.a?.P) {
      this.parent.markPositionStateUpdatedByWebsocket();
      this.parent.log(
        `ACCOUNT_UPDATE received with ${data.a.P.length} position(s): ${JSON.stringify(data.a.P)}`,
        LogSeverity.Info,
      );

      // Update positions immediately from WebSocket data for instant UI feedback
      data.a.P.forEach((pos: any) => {
        const positionAmount = parseFloat(pos.pa || '0');

        // In One-Way mode (ps === 'BOTH'), determine side from position amount
        // In Hedge mode (ps === 'LONG' or 'SHORT'), use the position side directly
        const positionSide = this.getPositionSide(pos, positionAmount);

        if (positionAmount !== 0) {
          // Only process positions with non-zero amount
          const existingPosition = this.parent.store.positions.find(
            (p) => p.symbol === pos.s && p.side === positionSide,
          );

          if (existingPosition) {
            this.parent.log(
              `Updating existing position: ${pos.s} ${positionSide} with contracts=${Math.abs(positionAmount)}`,
              LogSeverity.Info,
            );
            this.parent.store.updatePosition(existingPosition, {
              contracts: Math.abs(positionAmount),
              entryPrice: parseFloat(pos.ep || '0'),
              notional: Math.abs(positionAmount) * parseFloat(pos.ep || '0'),
              unrealizedPnl: parseFloat(pos.up || '0'),
              realizedPnl: parseFloat(pos.cr || '0'),
            });
          } else {
            this.parent.log(
              `Position not found in store: ${pos.s} ${positionSide}, will be created by fetchPositions()`,
              LogSeverity.Info,
            );

            // In One-Way mode, if we don't find the current side position,
            // we need to remove the opposite side position (position flipped from long to short or vice versa)
            if (pos.ps === 'BOTH') {
              const oppositeSide = positionSide === PositionSide.Long ? PositionSide.Short : PositionSide.Long;
              const oppositePosition = this.parent.store.positions.find(
                (p) => p.symbol === pos.s && p.side === oppositeSide,
              );

              if (oppositePosition) {
                this.parent.log(
                  `Removing opposite side position (flipped): ${pos.s} ${oppositeSide}`,
                  LogSeverity.Info,
                );
                // Remove the opposite position by updating the store
                this.parent.store.update({
                  positions: this.parent.store.positions.filter((p) => p.id !== oppositePosition.id),
                });
              }
            }
          }
        } else {
          // Position amount is zero - position was closed
          // In One-Way mode, we need to remove all positions for this symbol
          const positionsToRemove = this.parent.store.positions.filter((p) => p.symbol === pos.s);

          if (positionsToRemove.length > 0) {
            this.parent.log(`Removing ${positionsToRemove.length} closed position(s): ${pos.s}`, LogSeverity.Info);
            this.parent.store.update({
              positions: this.parent.store.positions.filter((p) => p.symbol !== pos.s),
            });
          }
        }
      });

      // Emit positions update immediately
      this.parent.emitter.emit('positions', this.parent.store.positions);

      this.schedulePositionRepair();
    }

    // ACCOUNT_UPDATE exposes wallet balance (wb) and cross wallet balance
    // (cw). For the USDT account collateral, cw is the available/free amount
    // and wb - cw is margin in use. Do not write assets as arbitrary root
    // keys; nested currencies retain the rest of the account state.
    if (this.applyBalanceUpdates(balances, upnlDeltas)) {
      this.scheduleBalanceReconcile(balanceStateVersion);
    }
  }

  private handleListenKeyExpired(_data: any): void {
    this.parent.log(
      'AsterDEX private listen key expired event received, reconnecting private stream',
      LogSeverity.Warning,
    );

    this.clearListenKeyRefreshTimer();
    this.clearListenKeyWatchdogTimer();
    this.reconnect();
  }

  private fetchListenKey = async (): Promise<string> => {
    // Call parent's API to get listen key
    const response = await this.parent.primaryAPI.post<{ listenKey: string }>(this.parent.getFapiPath('listenKey'));
    const listenKey = response.data.listenKey;
    this.listenKey = listenKey;
    this.listenKeyConsecutiveFailures = 0;
    this.lastListenKeyKeepAliveAt = Date.now();

    // Schedule refresh every 30 minutes (listen key is valid for 60 minutes)
    this.scheduleListenKeyRefresh(AsterDEXPrivateWebsocket.LISTEN_KEY_REFRESH_MS);
    this.startListenKeyWatchdog();

    return listenKey;
  };

  private updateListenKey = async () => {
    if (this.isDisposed) return;

    if (!this.listenKey) {
      this.parent.log('Missing AsterDEX listen key for keepalive, reconnecting private stream', LogSeverity.Warning);
      this.reconnect();
      this.scheduleListenKeyRefresh(AsterDEXPrivateWebsocket.LISTEN_KEY_FAILURE_RETRY_MS);
      return;
    }

    try {
      await this.parent.primaryAPI.put(this.parent.getFapiPath('listenKey'), undefined, {
        params: { listenKey: this.listenKey },
      });
      this.listenKeyConsecutiveFailures = 0;
      this.lastListenKeyKeepAliveAt = Date.now();
      this.scheduleListenKeyRefresh(AsterDEXPrivateWebsocket.LISTEN_KEY_REFRESH_MS);
    } catch (error: any) {
      this.listenKeyConsecutiveFailures++;
      const errorMsg = error?.message || 'Unknown error';
      const retryDelay = Math.min(
        5 * 60 * 1000,
        AsterDEXPrivateWebsocket.LISTEN_KEY_FAILURE_RETRY_MS * this.listenKeyConsecutiveFailures,
      );
      const shouldReconnect = /listenkey|expired|does not exist|-1125/i.test(errorMsg);

      this.parent.log(
        `Failed to refresh AsterDEX listen key (${this.listenKeyConsecutiveFailures}): ${errorMsg}. Retrying in ${Math.round(
          retryDelay / 1000,
        )}s`,
        LogSeverity.Warning,
      );

      if (shouldReconnect) {
        this.parent.log(
          'AsterDEX listen key appears expired/invalid, forcing private WebSocket reconnect',
          LogSeverity.Warning,
        );
        this.reconnect();
      }

      this.scheduleListenKeyRefresh(retryDelay);
    }
  };

  private scheduleListenKeyRefresh = (delayMs: number): void => {
    this.clearListenKeyRefreshTimer();
    this.listenKeyRefreshTimer = setTimeout(() => this.updateListenKey(), delayMs);
  };

  private clearListenKeyRefreshTimer = (): void => {
    if (this.listenKeyRefreshTimer) {
      clearTimeout(this.listenKeyRefreshTimer);
      this.listenKeyRefreshTimer = undefined;
    }
  };

  private startListenKeyWatchdog = (): void => {
    if (this.listenKeyWatchdogTimer || this.isDisposed) return;

    const checkAndSchedule = () => {
      if (this.isDisposed) return;

      if (this.lastListenKeyKeepAliveAt > 0) {
        const timeSinceKeepAlive = Date.now() - this.lastListenKeyKeepAliveAt;
        if (timeSinceKeepAlive > AsterDEXPrivateWebsocket.LISTEN_KEY_WATCHDOG_STALE_MS) {
          this.parent.log(
            `AsterDEX listen key keepalive stale for ${Math.round(timeSinceKeepAlive / 1000)}s, reconnecting private stream`,
            LogSeverity.Warning,
          );
          this.lastListenKeyKeepAliveAt = Date.now();
          this.reconnect();
        }
      }

      this.listenKeyWatchdogTimer = setTimeout(
        checkAndSchedule,
        AsterDEXPrivateWebsocket.LISTEN_KEY_WATCHDOG_INTERVAL_MS,
      );
    };

    this.listenKeyWatchdogTimer = setTimeout(
      checkAndSchedule,
      AsterDEXPrivateWebsocket.LISTEN_KEY_WATCHDOG_INTERVAL_MS,
    );
  };

  private clearListenKeyWatchdogTimer = (): void => {
    if (this.listenKeyWatchdogTimer) {
      clearTimeout(this.listenKeyWatchdogTimer);
      this.listenKeyWatchdogTimer = undefined;
    }
  };

  private scheduleConnectRetry = (): void => {
    if (this.isDisposed || this.connectRetryTimer) return;

    this.connectRetryCount++;
    const retryDelay = Math.min(60_000, 5_000 * this.connectRetryCount);
    this.parent.log(
      `Retrying AsterDEX private WebSocket initialization in ${Math.round(retryDelay / 1000)}s`,
      LogSeverity.Info,
    );

    this.connectRetryTimer = setTimeout(() => {
      this.connectRetryTimer = undefined;
      if (this.isDisposed) return;
      this.connectAndSubscribe();
    }, retryDelay);
  };

  private clearConnectRetryTimer = (): void => {
    if (this.connectRetryTimer) {
      clearTimeout(this.connectRetryTimer);
      this.connectRetryTimer = undefined;
    }
  };

  private schedulePositionRepair = (): void => {
    if (this.isDisposed) return;

    this.positionRepairQueued = true;

    if (this.positionRepairTimer || this.positionRepairInFlight) {
      return;
    }

    this.parent.log('Scheduling AsterDEX position repair (1s)...', LogSeverity.Debug);
    this.positionRepairTimer = setTimeout(() => {
      this.positionRepairTimer = undefined;
      void this.runPositionRepair();
    }, AsterDEXPrivateWebsocket.POSITION_REPAIR_DELAY_MS);
  };

  private runPositionRepair = async (): Promise<void> => {
    if (this.isDisposed || this.positionRepairInFlight) return;

    this.positionRepairQueued = false;
    this.positionRepairInFlight = true;
    const eventVersionAtStart = this.parent.getPositionStateVersion();

    try {
      const positions = await this.parent.fetchPositions();

      if (this.isDisposed) return;

      if (eventVersionAtStart === this.parent.getPositionStateVersion()) {
        this.parent.log('AsterDEX position repair completed successfully', LogSeverity.Debug);
        this.parent.store.addOrUpdatePositionsFull(positions);
        this.parent.store.update({ loaded: { ...this.parent.store.loaded, positions: true } });
      } else {
        this.parent.log(
          'Skipping stale AsterDEX position repair because a newer websocket position update arrived',
          LogSeverity.Debug,
        );
      }
    } catch (error: any) {
      this.parent.log(`Failed to repair AsterDEX positions after account update: ${error?.message || error}`, LogSeverity.Warning);
    } finally {
      this.positionRepairInFlight = false;

      if (this.positionRepairQueued && !this.isDisposed) {
        this.schedulePositionRepair();
      }
    }
  };

  private clearPositionRepairTimer = (): void => {
    if (this.positionRepairTimer) {
      clearTimeout(this.positionRepairTimer);
      this.positionRepairTimer = undefined;
    }
    this.positionRepairQueued = false;
  };

  private scheduleBalanceReconcile = (balanceStateVersion: number): void => {
    if (this.isDisposed) return;

    this.balanceReconcileQueued = true;
    if (this.balanceReconcileTimer || this.balanceReconcileInFlight) return;

    this.balanceReconcileTimer = setTimeout(() => {
      this.balanceReconcileTimer = undefined;
      void this.runBalanceReconcile(balanceStateVersion);
    }, AsterDEXPrivateWebsocket.POSITION_REPAIR_DELAY_MS);
  };

  private runBalanceReconcile = async (balanceStateVersion: number): Promise<void> => {
    if (this.isDisposed || this.balanceReconcileInFlight) return;

    this.balanceReconcileQueued = false;
    this.balanceReconcileInFlight = true;
    try {
      const applied = await this.parent.reconcileBalanceAfterPrivateUpdate(balanceStateVersion);
      if (!applied && !this.isDisposed) this.balanceReconcileQueued = true;
    } finally {
      this.balanceReconcileInFlight = false;
      if (this.balanceReconcileQueued && !this.isDisposed) {
        this.scheduleBalanceReconcile(this.parent.getBalanceStateVersion());
      }
    }
  };

  private clearBalanceReconcileTimer = (): void => {
    if (this.balanceReconcileTimer) {
      clearTimeout(this.balanceReconcileTimer);
      this.balanceReconcileTimer = undefined;
    }
    this.balanceReconcileQueued = false;
  };

  private scheduleOrderReconcile = (): void => {
    if (this.isDisposed) return;

    this.clearOrderReconcileTimer();
    this.parent.log('Scheduling debounced AsterDEX order reconcile (10s)...', LogSeverity.Debug);
    this.orderReconcileTimer = setTimeout(() => {
      this.orderReconcileTimer = undefined;
      void this.runOrderReconcile();
    }, AsterDEXPrivateWebsocket.ORDER_RECONCILE_DEBOUNCE_MS);
  };

  private runOrderReconcile = async (): Promise<void> => {
    if (this.isDisposed) return;

    try {
      const orders = await this.parent.fetchOrders();
      if (this.isDisposed) return;
      this.parent.log('AsterDEX order reconcile completed successfully', LogSeverity.Debug);
      this.parent.store.update({
        orders,
        loaded: {
          ...this.parent.store.loaded,
          orders: true,
        },
      });
    } catch (error: any) {
      this.parent.log(`Failed to reconcile AsterDEX orders after order event: ${error?.message || error}`, LogSeverity.Warning);
    }
  };

  private clearOrderReconcileTimer = (): void => {
    if (this.orderReconcileTimer) {
      clearTimeout(this.orderReconcileTimer);
      this.orderReconcileTimer = undefined;
    }
  };

  dispose = (): void => {
    this.listenKey = '';
    this.lastListenKeyKeepAliveAt = 0;
    this.listenKeyConsecutiveFailures = 0;
    this.connectRetryCount = 0;
    this.clearListenKeyRefreshTimer();
    this.clearListenKeyWatchdogTimer();
    this.clearConnectRetryTimer();
    this.clearPositionRepairTimer();
    this.clearBalanceReconcileTimer();
    this.clearOrderReconcileTimer();

    // Call parent dispose (regular prototype method, so super works here)
    super.dispose();
  };
}
