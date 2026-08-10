import type { BybitExchange } from './bybit.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { sleep } from '@tealstreet/sleep/src/sleep';
import { ENABLE_PWS_LOG, ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { OrderStatus, Position, Writable } from '../../types';
import { v } from '../../utils/get-key';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL, DEMO_WS_URL } from './bybit.types';

export class BybitPrivateWebsocket extends BaseWebSocket<BybitExchange> {
  private authRetries = 0;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(
        (this.parent.isDemo ? DEMO_WS_URL : BASE_WS_URL).private[this.parent.options.testnet ? 'testnet' : 'livenet'] +
          '?requestId=' +
          nanoid(),
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
      await this.auth();
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
      if (ENABLE_PWS_LOG) {
        this.parent.emitter.emit('grafana-pws', `${data}`);
      }

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
          this.emitAndLogError('[WS] Syncing time with Bybit...');
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

      for (const mappedOrder of orders) {
        this.parent.markOrderSeen(mappedOrder.id);
      }

      const price = parseFloat(v(order, 'price'));
      const amount = parseFloat(v(order, 'qty'));

      if (order.orderStatus === 'PartiallyFilled') {
        // False positive when order is replaced
        // it emits a partially filled with 0 amount & price
        if (price <= 0 && amount <= 0) return;
      }

      if (order.orderStatus === 'Filled' || order.orderStatus === 'PartiallyFilled') {
        this.emitter.emit('fill', {
          id: orders[0].id,
          side: orders[0].side,
          symbol: orders[0].symbol,
          price,
          amount,
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
        // Mutate the locally-owned `orders` array instead of `.map()`-ing
        // a clone with a status override. `mapOrder` returned this array
        // for our exclusive use; the next consumer (`addOrUpdateOrders`)
        // either mutates the existing store entry or pushes our object
        // into the store, so the orders we set here are the ones the store
        // will hold. Avoids 1–3 spread-clone allocations per terminal
        // order event.
        const newStatus = order.orderStatus === 'Filled' ? OrderStatus.Closed : OrderStatus.Canceled;
        for (let i = 0; i < orders.length; i++) {
          (orders[i] as Writable<(typeof orders)[number]>).status = newStatus;
        }
        this.store.addOrUpdateOrders(orders);
        // this.store.updateOrdersStatus(
        //   [{ id: orders[0].id }, { id: `${orders[0].id}__stop_loss` }, { id: `${orders[0].id}__take_profit` }],
        //   order.orderStatus === 'Filled' ? OrderStatus.Closed : OrderStatus.Canceled,
        // );
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
    // Arrow wrapper instead of `.bind(this)` — bind allocates a fresh
    // function object per call. Position events fire on every fill,
    // leverage change, and tick refresh; the alloc adds up under load.
    const positions: Position[] = data.map((p) => this.parent.mapPosition(p));
    this.store.addOrUpdatePositions(positions);
    this.parent.syncBalanceUpnlFromPositions();
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
