import type { Store } from '../../store/store.interface';
import type { AsterDEXExchange } from './asterdex.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { LogSeverity, OrderSide, OrderStatus, PositionSide } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { ASTERDEX_ENDPOINTS } from './asterdex.types';

type MessageHandlers = {
  [eventType: string]: Array<(data: any) => void>;
};

export class AsterDEXPrivateWebsocket extends BaseWebSocket<AsterDEXExchange> {
  private static readonly LISTEN_KEY_REFRESH_MS = 30 * 60 * 1000;
  private static readonly LISTEN_KEY_FAILURE_RETRY_MS = 30 * 1000;
  private static readonly LISTEN_KEY_WATCHDOG_INTERVAL_MS = 5 * 60 * 1000;
  private static readonly LISTEN_KEY_WATCHDOG_STALE_MS = 70 * 60 * 1000;

  private listenKey = '';
  private lastListenKeyKeepAliveAt = 0;
  private listenKeyConsecutiveFailures = 0;
  private listenKeyRefreshTimer?: NodeJS.Timeout;
  private listenKeyWatchdogTimer?: NodeJS.Timeout;
  private connectRetryTimer?: NodeJS.Timeout;
  private connectRetryCount = 0;

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

      // User Data Streams are accessed at /ws/<listenKey>
      // The base URL is wss://fstream.asterdex.com
      const baseUrl = ASTERDEX_ENDPOINTS.WS.PRIVATE.replace('/stream', '');
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
      this.parent.emitter.emit('fill', {
        orderId: orderData.i?.toString(),
        symbol: orderData.s,
        side: orderData.S === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
        price: parseFloat(orderData.ap || '0'),
        qty: parseFloat(orderData.l || '0'),
        timestamp: orderData.T || Date.now(),
      });
    }

    // Handle FILLED - update status to closed
    if (orderData.X === 'FILLED') {
      const order = this.parent.store.orders.find((o) => o.id === orderData.i?.toString());
      if (order) {
        this.parent.store.updateOrder(order, {
          status: OrderStatus.Closed,
          filled: parseFloat(orderData.z || '0'),
          remaining: 0,
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
      });

      this.parent.emitter.emit('order', order.id, order);
    }
  }

  private handleAccountUpdate(data: any): void {
    // Update account balances
    if (data.B) {
      const balances: Record<string, { free: number; used: number }> = {};

      data.B.forEach((balance: any) => {
        balances[balance.a] = {
          free: parseFloat(balance.f || '0'),
          used: parseFloat(balance.l || '0'),
        };
      });

      this.parent.store.update({
        balance: {
          ...this.parent.store.balance,
          ...balances,
        },
      });
    }

    this.parent.emitter.emit('account', data);
  }

  private handleAccountPositionUpdate(data: any): void {
    // Handle ACCOUNT_UPDATE event for futures positions
    if (data.a?.P) {
      this.parent.log(
        `ACCOUNT_UPDATE received with ${data.a.P.length} position(s): ${JSON.stringify(data.a.P)}`,
        LogSeverity.Info,
      );

      // Update positions immediately from WebSocket data for instant UI feedback
      data.a.P.forEach((pos: any) => {
        const positionAmount = parseFloat(pos.pa || '0');

        // In One-Way mode (ps === 'BOTH'), determine side from position amount
        // In Hedge mode (ps === 'LONG' or 'SHORT'), use the position side directly
        const positionSide =
          pos.ps === 'BOTH'
            ? positionAmount > 0
              ? PositionSide.Long
              : PositionSide.Short
            : pos.ps === 'LONG'
              ? PositionSide.Long
              : PositionSide.Short;

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

      // Trigger full position and order fetch after 1 second to ensure full sync
      // This handles new positions and ensures consistency
      // Also fetch orders since position changes often mean orders were filled
      this.parent.log('Scheduling delayed position and order fetch (1s)...', LogSeverity.Info);
      setTimeout(() => {
        if (!this.isDisposed) {
          // Fetch positions
          this.parent
            .fetchPositions()
            .then((positions) => {
              this.parent.log('Delayed position fetch completed successfully', LogSeverity.Info);
              // Update store with fetched positions
              this.parent.store.update({ positions });
            })
            .catch((error) => {
              this.parent.log(`Failed to fetch positions after update: ${error.message}`, LogSeverity.Warning);
            });

          // Also fetch orders since position changes often mean orders were filled
          this.parent
            .fetchOrders()
            .then((orders) => {
              this.parent.log('Delayed order fetch completed successfully', LogSeverity.Info);
              // Update store with fetched orders
              this.parent.store.update({ orders });
            })
            .catch((error) => {
              this.parent.log(`Failed to fetch orders after position update: ${error.message}`, LogSeverity.Warning);
            });
        }
      }, 1000);
    }

    // Update account balance
    if (data.a?.B) {
      data.a.B.forEach((balance: any) => {
        const asset = balance.a;
        const walletBalance = parseFloat(balance.wb);
        const crossWalletBalance = parseFloat(balance.cw);

        this.parent.store.update({
          balance: {
            ...this.parent.store.balance,
            [asset]: {
              free: walletBalance,
              used: walletBalance - crossWalletBalance,
            },
          },
        });
      });
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

  dispose = (): void => {
    this.listenKey = '';
    this.lastListenKeyKeepAliveAt = 0;
    this.listenKeyConsecutiveFailures = 0;
    this.connectRetryCount = 0;
    this.clearListenKeyRefreshTimer();
    this.clearListenKeyWatchdogTimer();
    this.clearConnectRetryTimer();

    // Call parent dispose (regular prototype method, so super works here)
    super.dispose();
  };
}
