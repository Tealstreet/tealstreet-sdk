import { MarginType, Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType, Position, PositionSide } from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { generateWsAuthSignature } from './backpack.signing';
import { BackpackExchange } from './backpack.exchange';
import { BackpackWsOrderUpdate, BackpackWsPositionUpdate, WSS_URLS } from './backpack.types';

export class BackpackPrivateWebSocket extends BaseWebSocket<BackpackExchange> {
  private subscriptions: Set<string> = new Set();

  // Throttled fetch state for positions
  private lastPositionFetchTime = 0;
  private positionFetchThrottleMs = 500;
  private positionFetchScheduled = false;
  private hasPendingPositionEvents = false;

  // Throttled fetch state for orders
  private ordersThrottleTimeout?: ReturnType<typeof setTimeout>;

  // Throttled fetch state for balance
  private lastBalanceFetchTime = 0;
  private balanceFetchThrottleMs = 500;
  private balanceFetchScheduled = false;
  private hasPendingBalanceEvents = false;

  constructor(parent: BackpackExchange) {
    super(parent);
  }

  connectAndSubscribe = async () => {
    if (!this.isDisposed) {
      const url = WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION'];
      this.ws = this.createWebsocket(url);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = async () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      await this.authenticate();
      this.resubscribe();
      this.ping();
    }
  };

  /**
   * Ping implementation for Backpack WebSocket
   * Send a ping message to keep the connection alive and measure latency.
   * Note: Backpack responds to PING with error code 4002 (parse error),
   * which we use as a pong response to measure latency.
   */
  ping = () => {
    if (!this.isDisposed && this.isConnected) {
      this.pingAt = performance.now();
      // Send ping message - Backpack responds with error 4002, which we use as pong
      this.send(JSON.stringify({ method: 'PING' }));
    }
  };

  private authenticate = async () => {
    try {
      const streams = ['account.orderUpdate', 'account.positionUpdate'];
      const authSig = await generateWsAuthSignature(
        this.parent.options.key,
        this.parent.options.secret,
        5000
      );

      this.send(
        JSON.stringify({
          method: 'SUBSCRIBE',
          params: streams,
          signature: authSig,
        })
      );

      streams.forEach((stream) => this.subscriptions.add(stream));
    } catch (error) {
      this.parent.emitAndLogError(`Failed to authenticate WebSocket: ${error}`);
    }
  };

  private resubscribe = async () => {
    // Re-authenticate on reconnection
    if (this.subscriptions.size > 0) {
      await this.authenticate();
    }
  };

  subscribeToOrderUpdates = async (symbol?: string) => {
    const stream = symbol ? `account.orderUpdate.${symbol}` : 'account.orderUpdate';

    if (!this.subscriptions.has(stream)) {
      try {
        const authSig = await generateWsAuthSignature(
          this.parent.options.key,
          this.parent.options.secret,
          5000
        );

        this.send(
          JSON.stringify({
            method: 'SUBSCRIBE',
            params: [stream],
            signature: authSig,
          })
        );

        this.subscriptions.add(stream);
      } catch (error) {
        this.parent.emitAndLogError(`Failed to subscribe to order updates: ${error}`);
      }
    }
  };

  subscribeToPositionUpdates = async () => {
    const stream = 'account.positionUpdate';

    if (!this.subscriptions.has(stream)) {
      try {
        const authSig = await generateWsAuthSignature(
          this.parent.options.key,
          this.parent.options.secret,
          5000
        );

        this.send(
          JSON.stringify({
            method: 'SUBSCRIBE',
            params: [stream],
            signature: authSig,
          })
        );

        this.subscriptions.add(stream);
      } catch (error) {
        this.parent.emitAndLogError(`Failed to subscribe to position updates: ${error}`);
      }
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        if (!json) return;

        // Handle subscription confirmations
        if (json.result === null && json.id) {
          return; // Subscription confirmation
        }

        // Handle error responses
        if (json.error) {
          // Parse errors (code 4002) come from our PING messages - use as pong response
          if (json.error.code === 4002) {
            this.handlePongEvent(10_000);
            return;
          }
          this.parent.emitAndLogError(`Private WebSocket error: ${JSON.stringify(json.error)}`);
          return;
        }

        // Handle data updates - messages come wrapped: { "stream": "...", "data": {...} }
        if (json.stream && json.data) {
          const stream = json.stream as string;

          if (stream === 'account.orderUpdate' || stream.startsWith('account.orderUpdate.')) {
            this.handleOrderUpdate(json.data as BackpackWsOrderUpdate);
          } else if (stream === 'account.positionUpdate') {
            this.handlePositionUpdate(json.data as BackpackWsPositionUpdate);
          }
        }
      } catch (error) {
        this.parent.emitAndLogError(`Error parsing private WebSocket message: ${error}`);
      }
    }
  };

  private handleOrderUpdate = (data: BackpackWsOrderUpdate) => {
    try {
      // Use O(1) hashmap lookup instead of O(n) find
      const market = this.store.marketsById[data.s];
      if (!market) return;

      const side = data.S === 'Bid' ? OrderSide.Buy : OrderSide.Sell;
      const triggerPrice = data.P ? parseFloat(data.P) : undefined;
      const isMarketOrder = data.o.toLowerCase() === 'market';

      // Determine order type - detect trigger orders (Market + triggerPrice or Market + reduceOnly + no price)
      let orderType: OrderType;
      const isTriggerOrder = triggerPrice !== undefined || (isMarketOrder && data.r === true && !data.p);

      if (isMarketOrder && isTriggerOrder) {
        // This is a trigger order - determine if it's TP or SL
        orderType = this.determineTriggerOrderType(market.symbol, side, triggerPrice);
      } else {
        orderType = this.mapOrderType(data.o);
      }

      // For trigger orders, use triggerPrice as display price; otherwise use p or L (last executed price)
      const price = triggerPrice ?? (data.p ? parseFloat(data.p) : (data.L ? parseFloat(data.L) : 0));

      // For trigger orders, use trigger quantity if available, otherwise fall back to regular quantity
      const rawAmount = isTriggerOrder && data.Q ? data.Q : data.q;
      const amount = parseFloat(rawAmount) || 0;
      const filled = parseFloat(data.z);

      const order: Order = {
        id: data.i,
        symbol: market.symbol,
        type: orderType,
        side,
        price,
        amount,
        filled,
        remaining: amount - filled,
        status: this.mapOrderStatus(data.X),
        lastUpdate: Math.floor(data.E / 1000), // Convert microseconds to milliseconds
        reduceOnly: data.r === true || isTriggerOrder,
        timeInForce: this.mapTimeInForce(data.f || 'GTC'),
      };

      this.store.addOrUpdateOrder(order);

      // Emit fill event for orderFill events
      if (data.e === 'orderFill' && data.l) {
        const fillAmount = parseFloat(data.l);
        const fillPrice = data.L ? parseFloat(data.L) : price;
        this.parent.emitter.emit('fill', {
          orderId: data.i,
          symbol: market.symbol,
          side: data.S === 'Bid' ? OrderSide.Buy : OrderSide.Sell,
          price: fillPrice,
          amount: fillAmount,
          timestamp: Math.floor(data.E / 1000),
        });

        // Trigger throttled position and balance fetch after fills
        this.throttledFetchPositions();
        this.throttledFetchBalance();
      }

      // Trigger throttled order fetch after any order update
      this.throttledFetchOrders();
    } catch (error) {
      this.parent.emitAndLogError(`Error handling order update: ${error}`);
    }
  };

  private handlePositionUpdate = async (data: BackpackWsPositionUpdate) => {
    try {
      // Use O(1) hashmap lookup instead of O(n) find
      const market = this.store.marketsById[data.s];
      if (!market) return;

      // Net quantity: positive = long, negative = short
      const netQuantity = parseFloat(data.q);
      const size = Math.abs(netQuantity);
      const side = netQuantity >= 0 ? PositionSide.Long : PositionSide.Short;
      const positionMode = await this.parent.getPositionMode();
      const positionId = createPositionId(market.symbol, side, positionMode);
      const oppositeSide = side === PositionSide.Long ? PositionSide.Short : PositionSide.Long;
      const oppositePositionId = createPositionId(market.symbol, oppositeSide, positionMode);

      if (size === 0) {
        // A zero net quantity has no side, so clear both internal side records.
        const updatedPositions = this.store.positions.filter(
          (position) => position.id !== positionId && position.id !== oppositePositionId,
        );
        if (updatedPositions.length !== this.store.positions.length) {
          this.store.update({ positions: updatedPositions });
        }
        return;
      }

      // Backpack reports one signed net quantity per symbol. Evict the stale
      // internal side before applying a flip that crosses through zero.
      if (this.store.positions.some((position) => position.id === oppositePositionId)) {
        this.store.update({ positions: this.store.positions.filter((position) => position.id !== oppositePositionId) });
      }

      // Entry price is in 'b' (breakeven) or 'B' field
      const entryPrice = parseFloat(data.b || data.B || '0');
      // Leverage from initial margin fraction: leverage = 1 / imf
      const imf = parseFloat(data.f) || 0.1;
      const leverage = imf > 0 ? Math.round(1 / imf) : 1;

      const position: Position = {
        id: positionId,
        symbol: market.symbol,
        side,
        entryPrice,
        notional: parseFloat(data.n) || Math.abs(size * entryPrice),
        leverage,
        unrealizedPnl: parseFloat(data.P),
        contracts: size,
        liquidationPrice: parseFloat(data.l) || 0,
        lastUpdate: Math.floor(data.E / 1000), // Convert microseconds to milliseconds
        // Backpack is always cross-margin hedged mode
        positionMode,
        marginType: MarginType.Cross,
      };

      this.store.addOrUpdatePosition(position);
    } catch (error) {
      this.parent.emitAndLogError(`Error handling position update: ${error}`);
    }
  };

  private mapOrderType = (backpackType: string): OrderType => {
    const type = backpackType.toLowerCase();
    switch (type) {
      case 'limit':
        return OrderType.Limit;
      case 'market':
        return OrderType.Market;
      default:
        return OrderType.Limit;
    }
  };

  /**
   * Determines if a trigger order is a StopLoss or TakeProfit based on the side
   * and trigger price relative to current market price.
   */
  private determineTriggerOrderType = (
    symbol: string,
    side: OrderSide,
    triggerPrice?: number
  ): OrderType => {
    // Get current market price from ticker
    const ticker = this.store.tickers.find((t) => t.symbol === symbol);
    const currentPrice = ticker?.last ?? ticker?.mark;

    if (!currentPrice || triggerPrice === undefined) {
      // If we don't have prices, default to StopLoss
      return OrderType.StopLoss;
    }

    if (side === OrderSide.Sell) {
      // Selling: below current = StopLoss (protect long), above = TakeProfit
      return triggerPrice < currentPrice ? OrderType.StopLoss : OrderType.TakeProfit;
    } else {
      // Buying: above current = StopLoss (protect short), below = TakeProfit
      return triggerPrice > currentPrice ? OrderType.StopLoss : OrderType.TakeProfit;
    }
  };

  private mapOrderStatus = (backpackStatus: string): OrderStatus => {
    switch (backpackStatus) {
      case 'New':
        return OrderStatus.Open;
      case 'PartiallyFilled':
        return OrderStatus.Open;
      case 'Filled':
        return OrderStatus.Closed;
      case 'Cancelled':
      case 'Expired':
        return OrderStatus.Canceled;
      default:
        return OrderStatus.Open;
    }
  };

  private mapTimeInForce = (backpackTif: string): OrderTimeInForce => {
    switch (backpackTif) {
      case 'GTC':
        return OrderTimeInForce.GoodTillCancel;
      case 'IOC':
        return OrderTimeInForce.ImmediateOrCancel;
      case 'FOK':
        return OrderTimeInForce.FillOrKill;
      case 'PO':
        return OrderTimeInForce.PostOnly;
      default:
        return OrderTimeInForce.GoodTillCancel;
    }
  };

  unsubscribe = (streams: string[]) => {
    if (streams.length > 0) {
      this.send(
        JSON.stringify({
          method: 'UNSUBSCRIBE',
          params: streams,
        })
      );

      streams.forEach((stream) => this.subscriptions.delete(stream));
    }
  };

  // Throttled position fetching - ensures we don't spam the API after rapid WebSocket events
  private throttledFetchPositions = () => {
    const now = Date.now();

    if (this.isDisposed) {
      return;
    }

    // Mark that we have pending events
    this.hasPendingPositionEvents = true;

    // If a fetch is already scheduled, let it handle the pending events
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
        if (!this.isDisposed && this.hasPendingPositionEvents) {
          this.executePositionFetch();
        }
      }, this.positionFetchThrottleMs - timeSinceLastFetch);
    }
  };

  private executePositionFetch = async () => {
    try {
      this.lastPositionFetchTime = Date.now();
      this.hasPendingPositionEvents = false;
      const positions = await this.parent.fetchPositions();
      if (!this.isDisposed) {
        this.store.update({ positions });
      }
    } catch {
      // Silent failure, will retry on next event
    }
  };

  // Trailing order reconciliation: the first event starts one 10s window.
  // Later events share that timer instead of postponing it indefinitely.
  private throttledFetchOrders = () => {
    if (this.ordersThrottleTimeout) {
      return;
    }
    this.ordersThrottleTimeout = setTimeout(this.fetchThrottledOrders, 10000);
  };

  private fetchThrottledOrders = async () => {
    this.ordersThrottleTimeout = undefined;
    if (!this.isDisposed) {
      try {
        const orders = await this.parent.fetchOrders();
        this.parent.store.update({
          orders,
          loaded: {
            ...this.parent.store.loaded,
            orders: true,
          },
        });
      } catch {
        // Silent failure, will retry on next event
      }
    }
  };

  // Throttled balance fetching - ensures we don't spam the API after rapid fill events
  private throttledFetchBalance = () => {
    const now = Date.now();

    if (this.isDisposed) {
      return;
    }

    // Mark that we have pending events
    this.hasPendingBalanceEvents = true;

    // If a fetch is already scheduled, let it handle the pending events
    if (this.balanceFetchScheduled) {
      return;
    }

    const timeSinceLastFetch = now - this.lastBalanceFetchTime;

    if (timeSinceLastFetch >= this.balanceFetchThrottleMs) {
      // Execute immediately if enough time has passed
      this.executeBalanceFetch();
    } else {
      // Schedule for later if we need to throttle
      this.balanceFetchScheduled = true;
      setTimeout(() => {
        this.balanceFetchScheduled = false;
        if (!this.isDisposed && this.hasPendingBalanceEvents) {
          this.executeBalanceFetch();
        }
      }, this.balanceFetchThrottleMs - timeSinceLastFetch);
    }
  };

  private executeBalanceFetch = async () => {
    try {
      this.lastBalanceFetchTime = Date.now();
      this.hasPendingBalanceEvents = false;
      const balance = await this.parent.fetchBalance();
      if (!this.isDisposed) {
        this.store.update({ balance });
      }
    } catch {
      // Silent failure, will retry on next event
    }
  };

  dispose = () => {
    this.subscriptions.clear();
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
      this.ordersThrottleTimeout = undefined;
    }
    if (this.ws) {
      this.ws.close();
    }
  };
}
