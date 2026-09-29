import type { BybitInverseExchange } from './bybitinverse.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { sleep } from '@tealstreet/sleep/src/sleep';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { resolveExchangeRouting } from '../../routing/resolveExchangeRouting';
import { WsDirectFallback } from '../../utils/wsDirectFallback';
import { isWsRelayEnabled } from '../../utils/wsRelay';
import { isRelayUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { OrderStatus, Position, Writable } from '../../types';
import { resolveBybitExecution } from './bybit.fill-event';
import { v } from '../../utils/get-key';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { getBybitWebSocketOptions } from './bybit.site';
import { BASE_WS_URL, DEMO_WS_URL } from './bybit.types';

export class BybitInversePrivateWebsocket extends BaseWebSocket<BybitInverseExchange> {
  private directFallback = new WsDirectFallback();

  private authRetries = 0;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const route = resolveExchangeRouting(this.parent.name, this.parent.options).wsPrivate;
      const directUrl = (this.parent.isDemo ? DEMO_WS_URL : BASE_WS_URL).private[
        this.parent.options.testnet ? 'testnet' : 'livenet'
      ];
      const baseUrl = this.directFallback.resolveUrl(
        route.tunnelled && route.host ? route.host : directUrl,
        isWsRelayEnabled() ? route.relay : undefined,
        route.fallback,
      );
      if (baseUrl === route.fallback || baseUrl === route.relay) {
        try {
          this.parent.emitter.emit('grafana-info', `Using tealstreet bybit private inverse ${baseUrl === route.relay ? 'relay' : 'proxy'}`);
        } catch (_e) {}
      }
      const webSocketOptions = this.parent.isDemo ? undefined : getBybitWebSocketOptions(this.parent.options);
      this.ws = this.createWebsocket(
        // The relay url already carries its query; Bybit's requestId is only cosmetic.
        isRelayUrl(baseUrl) ? baseUrl : baseUrl + '?requestId=' + nanoid(),
        undefined,
        webSocketOptions,
      );

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    this.sendReconnectSequence();
  };

  sendReconnectSequence = async (_syncTime = false) => {
    if (!this.isDisposed) {
      await virtualClock.syncBybitClientServerTime();
      await sleep(25);
      this.auth();
      this.subscribe();
      this.ping();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.parent.fetchAndSetPrivateSnapshot();
      }
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.directFallback.onMessage();
      this.emitWsDiagnosticReceive(data);

      const json = jsonParse(data);

      if (json?.topic === 'order') {
        this.recordMessageHandlerStats('order');
        this.handleOrderTopic(json.data);
      }

      if (json?.topic === 'position') {
        this.recordMessageHandlerStats('position');
        this.handlePositionTopic(json.data);
      }

      if (json?.op === 'pong') {
        this.handlePongEvent();
      }

      if (json?.success === false) {
        const msg: string[] = ['[WS]'];
        if (json?.op) {
          msg.push(`${json?.op}:`);
        }
        if (json?.ret_msg) {
          msg.push(json?.ret_msg);
        }
        this.emitAndLogError(msg.join(''));

        if (json?.op === 'auth' && this.authRetries <= 3) {
          this.emitAndLogError('[WS] Syncing time with BybitInverse...');
          this.authRetries++;
          this.sendReconnectSequence(true);
        }
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ op: 'ping' }));
    }
  };

  handleOrderTopic = (data: Array<Record<string, any>>) => {
    data.forEach((order: Record<string, any>) => {
      const orders = this.parent.mapOrder(order);

      for (const mapped of orders) {
        this.parent.markOrderSeen(mapped.id);
      }

      const price = parseFloat(v(order, 'price'));
      const amount = parseFloat(v(order, 'qty'));

      if (order.orderStatus === 'PartiallyFilled') {
        // False positive when order is replaced
        // it emits a partially filled with 0 amount & price
        if (price <= 0 && amount <= 0) return;
      }

      if (order.orderStatus === 'Filled' || order.orderStatus === 'PartiallyFilled') {
        // Deliberately NOT the `price`/`amount` above, which are the ORDER's
        // limit and size. See bybit.fill-event.ts. The replace guard keeps
        // reading the raw fields so its behaviour is unchanged.
        const executed = resolveBybitExecution(order);
        this.emitter.emit('fill', {
          id: orders[0].id,
          side: orders[0].side,
          symbol: orders[0].symbol,
          price: executed.price,
          amount: executed.amount,
        });
      }

      if (
        order.orderStatus === 'Cancelled' ||
        order.orderStatus === 'Filled' ||
        order.orderStatus === 'Deactivated' ||
        order.orderStatus === 'Rejected'
      ) {
        // We remove the order and its stop loss and take profit
        // if they exists, because they will be replaced with correct IDs
        if (
          order.orderStatus !== 'Filled' &&
          order.rejectReason &&
          // exclude EC_PerCancelRequest from error as thats simply user canceling the order
          order.rejectReason !== 'EC_PerCancelRequest' &&
          order.rejectReason !== 'EC_NoError'
        ) {
          this.emitAndLogError(order.rejectReason);
        }
        // Mutate locally-owned `orders` instead of `.map()`-cloning each
        // entry just to override status. Saves 1–3 spread allocations per
        // terminal order event.
        const newStatus = order.orderStatus === 'Filled' ? OrderStatus.Closed : OrderStatus.Canceled;
        for (let i = 0; i < orders.length; i++) {
          (orders[i] as Writable<(typeof orders)[number]>).status = newStatus;
        }
        this.store.addOrUpdateOrders(orders);
      }

      if (
        order.orderStatus === 'New' ||
        order.orderStatus === 'Untriggered' ||
        order.orderStatus === 'PartiallyFilled'
      ) {
        this.store.addOrUpdateOrders(orders);
      }
    });
  };

  handlePositionTopic = (data: Array<Record<string, any>>) => {
    // Arrow wrapper instead of `.bind(this.parent)` — bind allocates a
    // fresh function object per call.
    const positions: Position[] = data.map((p) => this.parent.mapPosition(p));
    const updates: Array<[Position, Position]> = positions.map((p) => [p, p]);
    this.store.updatePositions(updates);
    this.parent.lastWebsocketPositionsUpdate = virtualClock.getCurrentTimeMs();
  };

  private auth = async () => {
    const expires = virtualClock.getCurrentTimeMs() + 10_000;

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: nanoid(),
        type: 'ws-auth',
        exchangeName: 'bybit',
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: `GET/realtime${expires}`,
          outputFormat: 'hex',
          extraData: { expires },
        },
      });

      if (result.error) {
        this.emitAndLogError(`Remote WS auth failed: ${result.error}`);
        return;
      }

      this.sendWithLog(
        JSON.stringify({
          op: 'auth',
          args: [this.parent.options.key, expires.toFixed(0), result.wsAuthSignature],
        }),
      );
      return;
    }

    const signature = createHmac('sha256', this.parent.options.secret).update(`GET/realtime${expires}`).digest('hex');

    const payload = {
      op: 'auth',
      args: [this.parent.options.key, expires.toFixed(0), signature],
    };

    this.sendWithLog(JSON.stringify(payload));
  };

  private subscribe = () => {
    const payload = { op: 'subscribe', args: ['order', 'position'] };
    this.sendWithLog(JSON.stringify(payload));
  };
}
