import type { CoinbaseExchange } from './coinbase.exchange';

import {
  formatOrderState,
  formatOrderType,
  normalizeCfmContractSize,
  sign,
} from '@tealstreet/safe-cex/src/exchanges/coinbase/coinbase.utils';
import { Market, Order, OrderSide } from '@tealstreet/safe-cex/src/types';
import { ENABLE_PWS_LOG } from '../../config/config';
import { resolveExchangeRouting } from '../../routing/resolveExchangeRouting';
import { add, multiply } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL, ORDER_EVENT } from './coinbase.types';

const COINBASE_WS_AUTH_REQUEST_PREFIX = 'coinbase-ws-auth';
const COINBASE_USDC_ALIAS_EXCEPTIONS = new Set(['USDT-USDC', 'EURC-USDC']);

export class CoinbasePrivateWebsocket extends BaseWebSocket<CoinbaseExchange> {
  private subscriptionAttempt = 0;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const { key, secret, remoteSigner } = this.parent.options;
      if (!key || (!secret && !remoteSigner)) {
        this.emitAndLogError('Coinbase private WebSocket requires an API key and signing secret');
        return;
      }

      const route = resolveExchangeRouting(this.parent.name, this.parent.options).wsPrivate;
      this.ws = this.createWebsocket(
        route.tunnelled ? route.host! : BASE_WS_URL.private[this.parent.options.testnet ? 'testnet' : 'livenet'],
      );
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = async () => {
    if (this.isDisposed) return;
    const attempt = ++this.subscriptionAttempt;

    try {
      await this.subscribe(attempt);
      if (this.isDisposed || attempt !== this.subscriptionAttempt) return;
    } catch (err: any) {
      this.emitAndLogError(err?.message || 'Coinbase private WebSocket subscription signing failed');
      // A transient signing failure (remote signer offline, brief network blip)
      // must not permanently wedge the private WS. Close the socket so the base
      // reconnect path (with exponential backoff and failure-streak circuit
      // breaker) can retry, instead of `forceClose()` which sets
      // `forceClosed=true` and skips reconnect until an explicit `forceOpen()`.
      this.ws?.close();
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    this.trackLastReceived();
    if (!this.isDisposed) {
      if (ENABLE_PWS_LOG) {
        this.parent.emitter.emit('grafana-pws', `${data}`);
      }

      const json = JSON.parse(data);
      if (json.channel === 'user') {
        this.handleUserEvent(json);
      } else if (json.channel === 'heartbeats') {
        this.handlePongEvent();
      }
    }
  };

  // no ping for coinbase, only heartbeats
  ping = (): void => {};

  handleUserEvent = (data: Record<string, any>) => {
    const events = data?.events;
    if (events) {
      events.forEach((event: Record<string, any>) => {
        if (event.orders) {
          this.handleUserEventOrders(event as ORDER_EVENT);
        }
      });
    }
  };

  handleUserEventOrders = (data: ORDER_EVENT) => {
    const orders: Order[] = [];
    for (const order of data.orders) {
      const filledNative = parseFloat(order.cumulative_quantity);
      const remainingNative = parseFloat(order.leaves_quantity);
      const market = this.resolveOrderMarket(order.product_id);
      if (!market) {
        this.emitAndLogError(`Coinbase order skipped: missing market for ${order.product_id}`);
        continue;
      }
      const contractSize = market.isExpiring ? normalizeCfmContractSize(market.contractSize) : 1;
      if (contractSize === undefined) {
        this.emitAndLogError(`Coinbase CFM order skipped: invalid contract size for ${order.product_id}`);
        continue;
      }
      const filled = multiply(filledNative, contractSize);
      const remaining = multiply(remainingNative, contractSize);
      const amount = add(filled, remaining);
      orders.push({
        id: order.order_id,
        parentId: order.client_order_id,
        status: formatOrderState(order.status),
        symbol: market.symbol,
        type: formatOrderType(order.order_type),
        side: order.order_side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
        price: parseFloat(order.stop_price || order.limit_price || '0'),
        amount,
        filled,
        remaining,
        extra: {
          remainingNative,
        },
        reduceOnly: false,
        info: order,
        lastUpdate: virtualClock.getCurrentTimeMs(),
      });
    }
    // An empty upstream snapshot is authoritative and clears orders. A non-empty
    // snapshot that yielded no orders only contained quarantined records, so it
    // must not replace last-known-good state or mark orders as loaded.
    if (data.type === 'snapshot' && (data.orders.length === 0 || orders.length > 0)) {
      this.store.update({
        orders,
        loaded: { ...this.store.loaded, orders: true },
      });
    } else if (data.type === 'update') {
      this.store.addOrUpdateOrders(orders);
      // emit fills
      orders.forEach((order) => {
        if (order.info.status === 'FILLED') {
          this.emitter.emit('fill', {
            id: order.id,
            side: order.side,
            symbol: order.symbol,
            price: order.info.avg_price || 'market',
            amount: order.filled,
          });
        }
      });
    }
  };

  handlePositionTopic = (_data: Array<Record<string, any>>) => {};

  private resolveOrderMarket = (productId: string): Market | undefined => {
    const exact = this.store.marketsById[productId];
    if (exact) return exact;

    if (productId.endsWith('-USDC') && !COINBASE_USDC_ALIAS_EXCEPTIONS.has(productId)) {
      return this.store.marketsById[`${productId.slice(0, -5)}-USD`];
    }

    if (productId.endsWith('-USD')) {
      return this.store.marketsById[`${productId.slice(0, -4)}-USDC`];
    }

    return undefined;
  };

  private subscribe = async (attempt: number) => {
    for (const channel of ['user', 'heartbeats']) {
      const jwt = await this.buildSubscriptionJwt(channel);
      if (this.isDisposed || attempt !== this.subscriptionAttempt) return;
      this.sendWithLog(
        JSON.stringify({
          type: 'subscribe',
          channel,
          jwt,
        }),
      );
    }
  };

  private buildSubscriptionJwt = async (channel: string): Promise<string> => {
    const { key, secret, remoteSigner } = this.parent.options;
    if (remoteSigner) {
      const result = await remoteSigner.sign({
        id: `${COINBASE_WS_AUTH_REQUEST_PREFIX}.${channel}`,
        type: 'ws-auth',
        exchangeName: this.parent.name,
        accountName: remoteSigner.accountName,
        wsAuth: {
          toSign: `coinbase-websocket-jwt.${channel}`,
          outputFormat: 'coinbase-jwt',
          extraData: { channel },
        },
      });

      if (result.error || !result.wsAuthSignature) {
        throw new Error(result.error || 'Remote signer returned no Coinbase WebSocket JWT');
      }

      return result.wsAuthSignature;
    }

    return sign(key, secret);
  };
}
