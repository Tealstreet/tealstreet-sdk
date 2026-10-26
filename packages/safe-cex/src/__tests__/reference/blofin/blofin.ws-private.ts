import type { BlofinExchange } from './blofin.exchange';

import createHmac from 'create-hmac';
import sumBy from 'lodash/sumBy';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { OrderStatus } from '@tealstreet/safe-cex/src/types';
import { uuid } from '@tealstreet/safe-cex/src/utils/uuid';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { resolveTransportRouting } from '../../../transport/registry';
import { cleanFloat, createIncrementalFills } from '../../../utils/incrementalFills';
import { roundUSD } from '../../../utils/roundUSD';
import { multiply, subtract } from '../../../utils/safe-math';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, FETCH_POSITION_RESPONSE } from '../../../exchanges/blofin/blofin.types';

export class BlofinPrivateWebsocket extends BaseWebSocket<BlofinExchange> {
  // BloFin's private stream goes long idle stretches between user events and
  // has been observed to delay/drop `pong` echoes. Any inbound traffic here —
  // account/positions/orders updates, server pings, or actual pongs —
  // satisfies the keepalive; the 20s watchdog remains the backstop.
  protected override treatAnyMessageAsPong = true;

  // Throttled fetch state for orders
  private ordersThrottleTimeout?: ReturnType<typeof setTimeout>;
  // BloFin reports what an order has filled SO FAR; a fill event reports this execution.
  private readonly incrementalFills = createIncrementalFills();

  get endpoint() {
    return BASE_WS_URL.private[this.parent.options.testnet ? 'testnet' : 'livenet'];
  }

  dispose() {
    // Clear throttle timeout before calling parent dispose
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
      this.ordersThrottleTimeout = undefined;
    }
    super.dispose();
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      //this.ws = new WebSocket(this.endpoint);
      const route = resolveTransportRouting(this.parent.name, this.parent.options).wsPrivate;
      this.ws = this.createWebsocket(route.tunnelled ? route.host! : this.endpoint);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.auth();
      this.ping();
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog('ping');
    }
  };

  auth = async () => {
    const nonce = uuid();
    const timestamp = virtualClock.getCurrentTimeMs();

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: nanoid(),
        type: 'ws-auth',
        exchangeName: 'blofin',
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: `/users/self/verifyGET${timestamp}${nonce}`,
          outputFormat: 'hex-base64',
          extraData: { timestamp, nonce },
        },
      });

      if (result.error) {
        // NOT "auth failed": `health/errorPatterns.ts` matches that literal under
        // `auth_invalid`, which red-toasts "API key rejected" and makes
        // `noteTerminalExchangeFailure` suppress automatic account reloads for five
        // minutes — the very reload that would re-pair the browser signer. A
        // backgrounded tab or a 15s signer timeout must not look like a bad key.
        // Mirrors the bybit fix in `bybit/bybit.ws-private.ts` → auth.
        this.emitAndLogError(`Remote WS signing unavailable: ${result.error}`);
        return;
      }

      this.sendWithLog(
        JSON.stringify({
          op: 'login',
          args: [
            {
              apiKey: this.parent.options.key,
              passphrase: this.parent.options.passphrase,
              timestamp,
              sign: result.wsAuthSignature,
              nonce,
            },
          ],
        }),
      );
      return;
    }

    const signature = btoa(
      createHmac('sha256', this.parent.options.secret)
        .update(['/users/self/verify', 'GET', timestamp, nonce].join(''))
        .digest('hex'),
    );

    this.sendWithLog(
      JSON.stringify({
        op: 'login',
        args: [
          {
            apiKey: this.parent.options.key,
            passphrase: this.parent.options.passphrase,
            timestamp,
            sign: signature,
            nonce,
          },
        ],
      }),
    );
  };

  subscribe = () => {
    const args = [
      { channel: 'account' },
      // { channel: 'positions', instType: 'SWAP' },
      // { channel: 'orders', instType: 'SWAP' },
      // { channel: 'orders-algo', instType: 'SWAP' },
      { channel: 'positions' },
      { channel: 'orders' },
      { channel: 'orders-algo' },
    ];

    for (const arg of args) {
      this.sendWithLog(
        JSON.stringify({
          op: 'subscribe',
          args: [arg],
        }),
      );
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.emitWsDiagnosticReceive(data);

      this.logLastLog(data);

      if (data.includes('event":"subscribe"')) {
        return;
      }

      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

      if (data === '{"event":"login","code":"0","msg":""}') {
        this.subscribe();
        if (ENABLE_RECONNECTING_WEBSOCKET) {
          this.parent.fetchAndSetPrivateSnapshot();
        }
        return;
      }

      if (data.includes('"channel":"orders"') || data.includes('"channel":"orders-algo"')) {
        const json = jsonParse(data);
        if (json) this.handleOrderTopic(json);
        return;
      }

      if (data.includes('"channel":"positions"')) {
        const json = jsonParse<{ data: Array<Record<string, any>> }>(data);
        if (json) this.handlePositionTopic(json as { data: FETCH_POSITION_RESPONSE['data'] });
        return;
      }

      if (data.includes('"channel":"account"')) {
        const json = jsonParse<{ data: Array<Record<string, any>> }>(data);
        if (json) this.handleAccountTopic(json);
      }
    }
  };

  handleOrderTopic = ({ data: blofinOrders }: Record<string, any>) => {
    const latest = new Map(this.store.orders.map((order) => [order.id, order]));
    for (const o of blofinOrders) {
      const orders = this.parent.mapOrders([o]);
      const orderId = String(o.orderId || o.ordId || orders[0]?.id || '');

      if (orders.length) {
        if (o.state === 'filled') {
          this.store.markOrdersTerminal(orders, OrderStatus.Closed);
        }
        if (o.state === 'canceled') {
          this.store.markOrdersTerminal(orders, OrderStatus.Canceled);
        }
        if (o.state === 'effective') {
          this.store.markOrdersTerminal(orders, OrderStatus.Closed);
        }
        if (o.state === 'order_failed') {
          this.store.markOrdersTerminal(orders, OrderStatus.Canceled);
        }

        if (o.state === 'live' || o.state === 'partially_filled') {
          this.store.addOrUpdateOrders(
            orders.filter((order) => {
              const existing = latest.get(order.id);
              if (existing && order.filled < existing.filled) return false;
              latest.set(order.id, order);
              return true;
            }),
          );
        }

        if (o.state === 'filled' || o.state === 'partially_filled') {
          // Use O(1) hashmap lookup instead of O(n) find
          const market = this.store.marketsById[o.instId];
          const contractSize = market?.contractSize || parseFloat(market?.info?.contractValue || '1');
          const filledSizeNative = parseFloat(o.filledSize || '0');
          const averagePrice = parseFloat(o.averagePrice);

          if (market && Number.isFinite(filledSizeNative) && Number.isFinite(averagePrice) && averagePrice > 0) {
            // BloFin sends no per-fill value, but its average is full precision, so
            // filled x average is the running traded value (in contracts x price).
            const fill = this.incrementalFills(orderId, filledSizeNative, filledSizeNative * averagePrice);
            if (fill.qty > 0) {
              const rawTimestamp = Number(o.fillTime || o.uTime || o.cTime || Date.now());
              const timestamp =
                rawTimestamp > 1_000_000_000_000 ? Math.floor(rawTimestamp / 1000) : Math.floor(rawTimestamp);
              this.emitter.emit('fill', {
                id: orderId,
                orderId,
                side: orders[0].side,
                symbol: orders[0].symbol,
                // This execution's own price, not the order's running average.
                price: fill.value ? cleanFloat(fill.value / fill.qty) : averagePrice,
                amount: multiply(fill.qty, contractSize),
                timestamp,
              });
            }
          }
        }
      }
    }

    // Throttled fetch to ensure order state is synchronized after events
    this.throttledFetchOrders();
  };

  /**
   * Throttled fetch orders - debounces with 2 second timeout
   * This ensures we don't spam the API when multiple order events arrive
   * but still synchronize state after order activity settles
   */
  private throttledFetchOrders = () => {
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
    }
    this.ordersThrottleTimeout = setTimeout(this.fetchThrottledOrders, 2000);
  };

  private fetchThrottledOrders = async () => {
    this.ordersThrottleTimeout = undefined;
    if (!this.isDisposed) {
      try {
        const orders = await this.parent.fetchOrders();
        this.store.update({
          orders,
          loaded: {
            ...this.store.loaded,
            orders: true,
          },
        });
      } catch {
        // Silent failure, will retry on next event or tick
      }
    }
  };

  handlePositionTopic = ({ data: blofinPositions }: { data: FETCH_POSITION_RESPONSE['data'] }) => {
    // open positions
    const positions = this.parent.mapPositions(blofinPositions);
    this.store.addOrUpdatePositionsFull(positions);

    if (positions.length) {
      const used = roundUSD(sumBy(blofinPositions, (p) => parseFloat(p.maintenanceMargin)));
      const upnl = roundUSD(sumBy(blofinPositions, (p) => parseFloat(p.unrealizedPnl)));

      this.store.update({
        balance: { ...this.store.balance, used: used || 0, upnl: upnl || 0 },
      });
    }
  };

  handleAccountTopic = ({ data }: { data: Record<string, any> }) => {
    const totalCollateral = roundUSD(parseFloat(data.totalEquity));

    this.store.update({
      balance: {
        ...this.store.balance,
        free: subtract(totalCollateral, this.store.balance.used),
        total: subtract(totalCollateral, this.store.balance.upnl),
        info: data,
      },
    });
  };
}
