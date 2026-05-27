import type {
  CryptocomWsBalanceUpdate,
  CryptocomWsOrderUpdate,
  CryptocomWsPositionUpdate,
  CryptocomWsResponse,
} from './cryptocom.types';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType, Position, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { CryptocomExchange } from './cryptocom.exchange';
import { generateWsAuthMessage } from './cryptocom.signing';
import { WSS_URLS } from './cryptocom.types';

export class CryptocomPrivateWebSocket extends BaseWebSocket<CryptocomExchange> {
  private subscriptions: Set<string> = new Set();
  private messageId = 1;
  private isAuthenticated = false;

  constructor(parent: CryptocomExchange) {
    super(parent);
  }

  connectAndSubscribe = async () => {
    if (!this.isDisposed) {
      const urls = WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION'];
      this.ws = this.createWebsocket(urls.USER);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = async () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.isAuthenticated = false;
      await this.authenticate();
    }
  };

  private authenticate = async () => {
    try {
      const authMessage = generateWsAuthMessage(this.parent.options.key, this.parent.options.secret);

      this.send(JSON.stringify(authMessage));
    } catch (error) {
      this.parent.emitAndLogError(`Failed to authenticate WebSocket: ${error}`);
    }
  };

  private resubscribe = async () => {
    if (this.isAuthenticated && this.subscriptions.size > 0) {
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'subscribe',
          params: {
            channels: Array.from(this.subscriptions),
          },
          nonce: Date.now(),
        }),
      );
    }
  };

  subscribeToOrderUpdates = () => {
    const channel = 'user.order';

    if (!this.subscriptions.has(channel)) {
      this.subscriptions.add(channel);

      if (this.isAuthenticated) {
        this.send(
          JSON.stringify({
            id: this.messageId++,
            method: 'subscribe',
            params: {
              channels: [channel],
            },
            nonce: Date.now(),
          }),
        );
      }
    }
  };

  subscribeToPositionUpdates = () => {
    const channel = 'user.positions';

    if (!this.subscriptions.has(channel)) {
      this.subscriptions.add(channel);

      if (this.isAuthenticated) {
        this.send(
          JSON.stringify({
            id: this.messageId++,
            method: 'subscribe',
            params: {
              channels: [channel],
            },
            nonce: Date.now(),
          }),
        );
      }
    }
  };

  subscribeToBalanceUpdates = () => {
    const channel = 'user.balance';

    if (!this.subscriptions.has(channel)) {
      this.subscriptions.add(channel);

      if (this.isAuthenticated) {
        this.send(
          JSON.stringify({
            id: this.messageId++,
            method: 'subscribe',
            params: {
              channels: [channel],
            },
            nonce: Date.now(),
          }),
        );
      }
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        if (!json) return;

        // Auth ack still uses `code` + `method` with no `result`. Handle it
        // first because both the auth ack and data frames carry `code: 0`.
        if (
          'method' in json &&
          (json as CryptocomWsResponse).method === 'public/auth' &&
          (json as CryptocomWsResponse).code === 0
        ) {
          this.isAuthenticated = true;
          this.resubscribe();
          return;
        }

        // Data updates. As with the public WS, every data frame now carries
        // `code: 0` at the top level, so an unconditional early return on
        // `'code' in json` (the previous shape) silently drops all updates.
        if (json.result && json.result.channel) {
          const channel = json.result.channel;
          if (channel === 'user.order' || channel.startsWith('user.order')) {
            this.handleOrderUpdate(json.result as CryptocomWsOrderUpdate);
          } else if (channel === 'user.positions') {
            this.handlePositionUpdate(json.result as CryptocomWsPositionUpdate);
          } else if (channel === 'user.balance') {
            this.handleBalanceUpdate(json.result as CryptocomWsBalanceUpdate);
          }
          return;
        }

        if ('code' in json) {
          const response = json as CryptocomWsResponse;
          if (response.code !== 0) {
            this.parent.emitAndLogError(
              `Private WebSocket error ${response.code}: ${response.message || 'Unknown error'}`,
            );
          }
          return;
        }
      } catch (error) {
        this.parent.emitAndLogError(`Error parsing private WebSocket message: ${error}`);
      }
    }
  };

  private handleOrderUpdate = (update: CryptocomWsOrderUpdate) => {
    try {
      const orderData = update.data[0];
      if (!orderData) return;

      const market = this.store.markets.find((m) => m.id === orderData.instrument_name);
      if (!market) return;

      // v1 numerics are strings; status/timestamps use either the new
      // (status / create_time / update_time) or legacy (order_status /
      // time_created / time_updated) names depending on the WS channel.
      const quantity = parseFloat(orderData.quantity) || 0;
      const filled = orderData.cumulative_quantity ? parseFloat(orderData.cumulative_quantity) || 0 : 0;
      const price = orderData.limit_price ? parseFloat(orderData.limit_price) || 0 : 0;
      const status = orderData.status ?? orderData.order_status ?? 'ACTIVE';
      const updated = orderData.update_time ?? orderData.time_updated;
      const created = orderData.create_time ?? orderData.time_created;

      const order: Order = {
        id: orderData.order_id,
        symbol: market.symbol,
        type: this.mapOrderType(orderData.order_type),
        side: orderData.side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
        price,
        amount: quantity,
        filled,
        remaining: Math.max(0, quantity - filled),
        status: this.mapOrderStatus(status),
        lastUpdate: updated || created || virtualClock.getCurrentTimeMs(),
        reduceOnly: false,
        timeInForce: this.mapTimeInForce(orderData.time_in_force),
      };

      this.store.addOrUpdateOrder(order);
    } catch (error) {
      this.parent.emitAndLogError(`Error handling order update: ${error}`);
    }
  };

  private handlePositionUpdate = async (update: CryptocomWsPositionUpdate) => {
    try {
      const positionData = update.data[0];
      if (!positionData) return;

      const market = this.store.markets.find((m) => m.id === positionData.instrument_name);
      if (!market) return;

      // v1 numerics are strings. Side is encoded in the sign of `quantity`
      // (negative = short). Legacy frames still set `type` to
      // LONG_POSITION/SHORT_POSITION, but quantity-sign is the source of truth
      // — using the type field on v1 always produced `Short` because the
      // field is now constant `PERPETUAL_SWAP`.
      const quantity = parseFloat(positionData.quantity) || 0;
      const cost = parseFloat(positionData.cost) || 0;
      const positionMode = await this.parent.getPositionMode();

      if (quantity === 0) {
        // Position closed — drop *all* rows for this instrument. quantity === 0
        // carries no side info (and the legacy type field is now constant
        // `PERPETUAL_SWAP`), so we can't recover which side closed from this
        // frame alone. Filtering by symbol is safe: a close means there is no
        // open exposure on either side for that instrument anymore.
        const before = this.store.positions.length;
        const updatedPositions = this.store.positions.filter((p) => p.symbol !== market.symbol);
        if (updatedPositions.length !== before) {
          this.store.update({ positions: updatedPositions });
        }
        return;
      }

      const side = quantity > 0 ? PositionSide.Long : PositionSide.Short;
      const positionId = createPositionId(market.symbol, side, positionMode);

      const contracts = Math.abs(quantity);
      const entryPrice = cost / contracts || 0;

      const position: Position = {
        id: positionId,
        symbol: market.symbol,
        side,
        entryPrice,
        notional: Math.abs(cost),
        leverage: 1,
        unrealizedPnl: parseFloat(positionData.open_position_pnl) || 0,
        contracts,
        liquidationPrice: 0,
        lastUpdate: positionData.update_timestamp_ms || virtualClock.getCurrentTimeMs(),
      };

      this.store.addOrUpdatePosition(position);
    } catch (error) {
      this.parent.emitAndLogError(`Error handling position update: ${error}`);
    }
  };

  private handleBalanceUpdate = (_update: CryptocomWsBalanceUpdate) => {
    // Handle balance updates if needed
    // This can trigger a balance refresh
  };

  private mapOrderType = (cryptocomType: string): OrderType => {
    switch (cryptocomType) {
      case 'MARKET':
        return OrderType.Market;
      case 'LIMIT':
        return OrderType.Limit;
      case 'STOP_LOSS':
      case 'STOP_LIMIT':
        return OrderType.StopLoss;
      case 'TAKE_PROFIT':
      case 'TAKE_PROFIT_LIMIT':
        return OrderType.TakeProfit;
      default:
        return OrderType.Limit;
    }
  };

  private mapOrderStatus = (cryptocomStatus: string): OrderStatus => {
    switch (cryptocomStatus) {
      case 'ACTIVE':
      case 'PENDING':
        return OrderStatus.Open;
      case 'FILLED':
        return OrderStatus.Closed;
      case 'CANCELED':
      case 'EXPIRED':
      case 'REJECTED':
        return OrderStatus.Canceled;
      default:
        return OrderStatus.Open;
    }
  };

  private mapTimeInForce = (cryptocomTif: string): OrderTimeInForce => {
    switch (cryptocomTif) {
      case 'IMMEDIATE_OR_CANCEL':
        return OrderTimeInForce.ImmediateOrCancel;
      case 'FILL_OR_KILL':
        return OrderTimeInForce.FillOrKill;
      case 'POST_ONLY':
        return OrderTimeInForce.PostOnly;
      default:
        return OrderTimeInForce.GoodTillCancel;
    }
  };

  unsubscribe = (channels: string[]) => {
    if (channels.length > 0) {
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'unsubscribe',
          params: {
            channels,
          },
          nonce: Date.now(),
        }),
      );

      channels.forEach((channel) => this.subscriptions.delete(channel));
    }
  };

  dispose = () => {
    this.subscriptions.clear();
    this.isAuthenticated = false;
    if (this.ws) {
      this.ws.close();
    }
  };
}
