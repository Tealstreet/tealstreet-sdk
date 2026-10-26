import type { BinanceSpotExchange } from './binancespot.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import createHmac from 'create-hmac';
import { OrderStatus, OrderType } from '../../../types';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ORDER_SIDE, ORDER_TYPE_SPOT, USER_DATA_RECV_WINDOW } from '../../../exchanges/binance/binancespot.types';

export const BINANCE_SPOT_USER_DATA_REQUEST_ID = 'tealstreet-user-data';
export const BINANCE_SPOT_HEARTBEAT_REQUEST_ID = 42;

export const buildBinanceSpotUserDataSignaturePayload = (
  apiKey: string,
  timestamp: number,
  recvWindow = USER_DATA_RECV_WINDOW,
) => `apiKey=${apiKey}&recvWindow=${recvWindow}&timestamp=${timestamp}`;

export class BinancePrivateWebsocket extends BaseWebSocket<BinanceSpotExchange> {
  private subscriptionAttempt = 0;

  connectAndSubscribe = async () => {
    // payloadSigner signs REST only; auth here needs the local secret.
    if (this.parent.options.payloadSigner) return;
    if (this.isDisposed) return;

    const { key: apiKey, secret, remoteSigner } = this.parent.options;
    if (!apiKey || (!secret && !remoteSigner)) {
      this.emitAndLogError('Binance Spot private WebSocket requires an API key and signing secret');
      return;
    }

    const environment = this.parent.options.testnet ? 'testnet' : 'livenet';
    this.ws = this.createWebsocket(BASE_WS_URL.private[environment]);
    this.ws.addEventListener('message', this.onMessage);
    this.ws.addEventListener('close', this.onClose);
    this.ws.addEventListener('open', this.onOpen);
  };

  onOpen = async () => {
    if (this.isDisposed) return;
    const attempt = ++this.subscriptionAttempt;

    try {
      const params = await this.buildUserDataSubscriptionParams();
      // A hosted signer may resolve after the transport has already reopened.
      // Never let that stale signature subscribe the newer connection twice.
      if (this.isDisposed || attempt !== this.subscriptionAttempt) return;

      this.sendWithLog(
        JSON.stringify({
          id: BINANCE_SPOT_USER_DATA_REQUEST_ID,
          method: 'userDataStream.subscribe.signature',
          params,
        }),
      );
    } catch (err: any) {
      this.emitAndLogError(err?.message || 'Binance Spot user-data subscription signing failed');
      this.forceClose();
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.emitWsDiagnosticReceive(data);

      const json = jsonParse(data);
      if (!json) return;

      if (json.id === BINANCE_SPOT_USER_DATA_REQUEST_ID) {
        if (json.status !== 200 || typeof json.result?.subscriptionId !== 'number') {
          const reason = json.error?.msg || `subscription rejected with status ${json.status ?? 'unknown'}`;
          this.emitAndLogError(`Binance Spot user-data subscription failed: ${reason}`);
          this.forceClose();
          return;
        }

        this.ping();
        return;
      }

      // WebSocket API user-data events are wrapped as
      // `{ subscriptionId, event: { e: ... } }`, unlike retired listen-key
      // streams which delivered the event object directly.
      const event = json.event || json;

      if (event.e === 'outboundAccountPosition') {
        this.recordMessageHandlerStats('position');
        this.handleAccountEvents([event]);
      }
      if (event.e === 'executionReport') {
        this.recordMessageHandlerStats('order');
        this.handleOrderEvents([event]);
      }

      if (json.id === BINANCE_SPOT_HEARTBEAT_REQUEST_ID) {
        if (json.status === 200 && json.result?.userDataStream === true) {
          this.handlePongEvent();
        } else {
          this.emitAndLogError('Binance Spot user-data subscription is no longer active; reconnecting');
          void this.reconnect();
        }
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ id: BINANCE_SPOT_HEARTBEAT_REQUEST_ID, method: 'session.status' }));
    }
  };

  private buildUserDataSubscriptionParams = async () => {
    const apiKey = this.parent.options.key;
    const timestamp = virtualClock.getCurrentTimeMs();
    const payload = buildBinanceSpotUserDataSignaturePayload(apiKey, timestamp);

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: BINANCE_SPOT_USER_DATA_REQUEST_ID,
        type: 'ws-auth',
        exchangeName: 'binancespot',
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: payload,
          outputFormat: 'hex',
          extraData: { apiKey, recvWindow: USER_DATA_RECV_WINDOW, timestamp },
        },
      });

      if (result.error || !result.wsAuthSignature) {
        throw new Error(result.error || 'Remote signer returned no Binance Spot WebSocket signature');
      }

      return { apiKey, recvWindow: USER_DATA_RECV_WINDOW, timestamp, signature: result.wsAuthSignature };
    }

    const signature = createHmac('sha256', this.parent.options.secret).update(payload).digest('hex');
    return { apiKey, recvWindow: USER_DATA_RECV_WINDOW, timestamp, signature };
  };

  handleOrderEvents = (events: Array<Record<string, any>>) => {
    events.forEach((data) => {
      // Spot `executionReport`: `x` is the execution type (NEW/TRADE/CANCELED/…),
      // `X` is the order status (NEW/PARTIALLY_FILLED/FILLED/CANCELED/…). Fills
      // arrive with `x === 'TRADE'`.
      if (data.x === 'TRADE') {
        this.parent.emitter.emit('fill', {
          id: data.c,
          side: ORDER_SIDE[data.S],
          symbol: data.s,
          price: parseFloat(data.p),
          amount: parseFloat(data.l),
        });

        // Mirror the cumulative fill (`z`) onto the stored order so a partially
        // or fully filled order reflects its real filled/remaining before the
        // next REST fetchOrders — same reason as futures (chaser reads
        // order.filled from the store). See binance.ws-private.ts.
        const orderId = data.c;
        const filled = parseFloat(data.z);
        const amount = parseFloat(data.q);
        this.store.updateOrder(
          { id: orderId },
          {
            filled,
            remaining: amount - filled,
            lastUpdate: virtualClock.getCurrentTimeMs(),
          },
        );

        // Fully filled (`z === q`, or order status FILLED) → transition out of Open.
        if (data.X === 'FILLED' || filled >= amount) {
          this.store.updateOrderStatus({ id: orderId }, OrderStatus.Closed);
        }
      }

      if (data.X === 'NEW') {
        this.store.addOrUpdateOrder({
          id: data.c,
          status: OrderStatus.Open,
          symbol: data.s,
          // Spot reports the order type in `o` (LIMIT/STOP_LOSS_LIMIT/LIMIT_MAKER/…),
          // NOT the futures `ot` field. Spot has no reduceOnly concept.
          type: ORDER_TYPE_SPOT[data.o] || OrderType.Limit,
          side: ORDER_SIDE[data.S],
          price: parseFloat(data.p) || parseFloat(data.sp),
          amount: parseFloat(data.q),
          filled: parseFloat(data.z),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: parseFloat(data.q) - parseFloat(data.z),
          reduceOnly: false,
          info: data,
        });
      }

      if (data.X === 'CANCELED' || data.X === 'REJECTED' || data.X === 'EXPIRED') {
        this.store.updateOrderStatus({ id: data.c }, OrderStatus.Canceled);
      }
    });
  };

  handleAccountEvents = (events: Array<Record<string, any>>) => {
    events.forEach((event) => {
      const currencies = { ...(this.store.balance.currencies || {}) };
      const coins = { ...(this.store.balance.coins || {}) };

      for (const assetBalance of event.B || []) {
        const asset = assetBalance.a;
        const free = parseFloat(assetBalance.f || '0');
        const used = parseFloat(assetBalance.l || '0');
        const total = free + used;
        const price = this.parent.getAssetUsdPrice(asset);

        currencies[asset] = { free, used, total, upnl: 0, info: assetBalance };
        coins[asset] = { base: asset, amount: total, notional: total * price };
      }

      let free = 0;
      let used = 0;
      let total = 0;
      for (const [asset, balance] of Object.entries(currencies)) {
        const price = this.parent.getAssetUsdPrice(asset);
        free += balance.free * price;
        used += balance.used * price;
        total += balance.total * price;
      }

      this.store.update({
        balance: { free, used, total, upnl: 0, currencies, coins, info: event },
      });
    });
  };
}
