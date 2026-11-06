import type {
  CryptocomWsBalanceUpdate,
  CryptocomWsOrderUpdate,
  CryptocomWsPositionUpdate,
  CryptocomWsResponse,
} from './cryptocom.types';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import {
  Balance,
  MarginType,
  Order,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  Position,
  PositionSide,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { CryptocomExchange } from './cryptocom.exchange';
import { generateWsAuthMessage } from './cryptocom.signing';
import {
  CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS,
  CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS,
  WSS_URLS,
} from './cryptocom.types';

export class CryptocomPrivateWebSocket extends BaseWebSocket<CryptocomExchange> {
  protected override treatAnyMessageAsPong = true;
  private subscriptions: Set<string> = new Set();
  private messageId = 1;
  private isAuthenticated = false;
  private reconnectAuthTimer?: ReturnType<typeof setTimeout>;

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

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.isAuthenticated = false;
      clearTimeout(this.reconnectAuthTimer);
      this.reconnectAuthTimer = setTimeout(() => {
        if (!this.isDisposed) void this.authenticate();
      }, 1000);
    }
  };

  protected shouldEnforcePongMissReconnect = (): boolean => false;

  getWatchdogStaleThresholdMs = (): number => CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS;

  getWatchdogPingThresholdMs = (): number => CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS;

  ping = (): void => {};

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
          nonce: virtualClock.getCurrentTimeMs(),
        }),
      );
    }
  };

  subscribeToOrderUpdates = () => {
    const channels = ['user.order', 'user.advance.order'];
    const newChannels = channels.filter((channel) => !this.subscriptions.has(channel));
    for (const channel of newChannels) this.subscriptions.add(channel);

    if (this.isAuthenticated && newChannels.length > 0) {
      this.send(
        JSON.stringify({
          id: this.messageId++,
          method: 'subscribe',
          params: { channels: newChannels },
          nonce: virtualClock.getCurrentTimeMs(),
        }),
      );
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
            nonce: virtualClock.getCurrentTimeMs(),
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
            nonce: virtualClock.getCurrentTimeMs(),
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

        if (json.method === 'public/heartbeat' && typeof json.id === 'number') {
          this.send(JSON.stringify({ id: json.id, method: 'public/respond-heartbeat' }));
          return;
        }

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
          this.recordMessageHandlerStats(channel);
          if (channel.startsWith('user.order') || channel.startsWith('user.advance.order')) {
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
      for (let i = 0; i < update.data.length; i++) {
        const orderData = update.data[i];
        const market = this.store.marketsById[orderData.instrument_name];
        if (!market) continue;

        // v1 numerics are strings; status/timestamps use either the new
        // (status / create_time / update_time) or legacy (order_status /
        // time_created / time_updated) names depending on the WS channel.
        const quantity = parseFloat(orderData.quantity) || 0;
        const filled = orderData.cumulative_quantity ? parseFloat(orderData.cumulative_quantity) || 0 : 0;
        const price = parseFloat(orderData.ref_price ?? orderData.trigger_price ?? orderData.limit_price ?? '0') || 0;
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
          reduceOnly: orderData.exec_inst?.includes('REDUCE_ONLY') ?? false,
          timeInForce: this.mapTimeInForce(orderData.time_in_force),
        };

        this.store.addOrUpdateOrder(order);
      }
    } catch (error) {
      this.parent.emitAndLogError(`Error handling order update: ${error}`);
    }
  };

  private handlePositionUpdate = async (update: CryptocomWsPositionUpdate) => {
    try {
      const positionMode = await this.parent.getPositionMode();
      for (let i = 0; i < update.data.length; i++) {
        const positionData = update.data[i];
        const market = this.store.marketsById[positionData.instrument_name];
        if (!market) continue;

        // Side is encoded in the sign of quantity (negative = short).
        const quantity = parseFloat(positionData.quantity) || 0;
        const cost = parseFloat(positionData.cost) || 0;

        if (quantity === 0) {
          const before = this.store.positions.length;
          const updatedPositions = this.store.positions.filter((p) => p.symbol !== market.symbol);
          if (updatedPositions.length !== before) this.store.update({ positions: updatedPositions });
          continue;
        }

        const side = quantity > 0 ? PositionSide.Long : PositionSide.Short;
        const contracts = Math.abs(quantity);
        const entryPrice = cost / contracts || 0;
        const leverage = parseFloat(positionData.leverage ?? positionData.target_leverage ?? '1') || 1;
        const liquidationPrice = parseFloat(positionData.liquidation_price ?? '0') || 0;
        const markPrice = parseFloat(positionData.mark_price ?? '0') || 0;

        const position: Position = {
          id: createPositionId(market.symbol, side, positionMode),
          symbol: market.symbol,
          side,
          entryPrice,
          notional: Math.abs(cost),
          leverage,
          marginType: positionData.isolation_type === 'ISOLATED_MARGIN' ? MarginType.Isolated : MarginType.Cross,
          positionMode,
          unrealizedPnl: parseFloat(positionData.open_position_pnl) || 0,
          contracts,
          liquidationPrice,
          info: { markPrice, isolationId: positionData.isolation_id },
          lastUpdate: positionData.update_timestamp_ms || virtualClock.getCurrentTimeMs(),
        };

        this.store.addOrUpdatePosition(position);
      }
    } catch (error) {
      this.parent.emitAndLogError(`Error handling position update: ${error}`);
    }
  };

  private handleBalanceUpdate = (update: CryptocomWsBalanceUpdate) => {
    const summary = update.data[0];
    if (!summary) return;

    const currencies: NonNullable<Balance['currencies']> = {};
    for (let i = 0; i < (summary.position_balances || []).length; i++) {
      const row = summary.position_balances[i];
      const quantity = parseFloat(row.quantity) || 0;
      const available = parseFloat(row.max_withdrawal_balance) || 0;
      currencies[row.instrument_name] = {
        total: quantity,
        free: available,
        used: Math.max(0, quantity - available),
        upnl: 0,
      };
    }

    this.store.update({
      balance: {
        total: parseFloat(summary.total_margin_balance) || 0,
        free: parseFloat(summary.total_available_balance) || 0,
        used: parseFloat(summary.total_initial_margin) || 0,
        upnl: parseFloat(summary.total_session_unrealized_pnl) || 0,
        currencies,
      },
      loaded: { ...this.store.loaded, balance: true },
    });
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
          nonce: virtualClock.getCurrentTimeMs(),
        }),
      );

      channels.forEach((channel) => this.subscriptions.delete(channel));
    }
  };

  dispose = () => {
    clearTimeout(this.reconnectAuthTimer);
    this.subscriptions.clear();
    this.isAuthenticated = false;
    if (this.ws) {
      this.ws.close();
    }
  };
}
