import type { BybitExchange } from './bybit.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { sleep } from '@tealstreet/sleep/src/sleep';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { getTransport, resolveTransportRouting } from '../../../transport/registry';
import { isWsRelayEnabled } from '../../../transport/tealstreet/wsRelay';
import { OrderStatus, Position, Writable } from '../../../types';
import { v } from '../../../utils/get-key';
import { cleanFloat, createIncrementalFills } from '../../../utils/incrementalFills';
import { virtualClock } from '../../../utils/virtualClock';
import { WsDirectFallback } from '../../../utils/wsDirectFallback';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { resolveBybitExecution } from './bybit.fill-event';
import { getBybitWebSocketOptions } from '../../../exchanges/bybit/bybit.site';
import { BASE_WS_URL, DEMO_WS_URL } from '../../../exchanges/bybit/bybit.types';

export class BybitPrivateWebsocket extends BaseWebSocket<BybitExchange> {
  // Bybit reports what an order has filled SO FAR; a fill event reports this execution.
  private readonly incrementalFills = createIncrementalFills();

  private directFallback = new WsDirectFallback();

  private authRetries = 0;

  connectAndSubscribe = () => {
    // payloadSigner signs REST only; auth here needs the local secret.
    if (this.parent.options.payloadSigner) return;
    if (!this.isDisposed) {
      const network = this.parent.options.testnet ? 'testnet' : 'livenet';
      const route = resolveTransportRouting(this.parent.name, this.parent.options).wsPrivate;
      const directUrl =
        this.parent.getBybitPrivateWsUrl?.(network) ??
        (this.parent.isDemo ? DEMO_WS_URL : BASE_WS_URL).private[network];
      const baseUrl = this.directFallback.resolveUrl(
        route.tunnelled && route.host ? route.host : directUrl,
        isWsRelayEnabled() ? route.relay : undefined,
        route.fallback,
      );
      if (baseUrl === route.fallback || baseUrl === route.relay) {
        try {
          this.parent.emitter.emit(
            'grafana-info',
            `Using tealstreet bybit private ${baseUrl === route.relay ? 'relay' : 'proxy'}`,
          );
        } catch (_e) {}
      }
      const webSocketOptions = this.parent.isDemo ? undefined : getBybitWebSocketOptions(this.parent.options);
      this.ws = this.createWebsocket(
        // The relay url already carries its query; Bybit's requestId is only cosmetic.
        getTransport(this.parent.options).wsConnection(baseUrl)?.kind === 'relay'
          ? baseUrl
          : baseUrl + '?requestId=' + nanoid(),
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
      let authed = false;
      try {
        authed = await this.auth();
      } catch (error) {
        // `remoteSigner.sign` THROWS — signer unavailable, the 15s sign timeout, a
        // failed send, or `markUnavailable` rejecting everything pending — it does
        // NOT resolve `{ error }`, which is the only case `auth` handles. Pass the
        // error ALONE: for an Error input a second argument REPLACES the message
        // (`base.ts`), so naming it here would destroy the only description of the
        // cause and collapse four distinct failures into one line.
        this.emitAndLogError(error instanceof Error ? error : new Error(String(error)));
      }

      if (!authed) {
        // Send NOTHING. Bybit refuses a private subscribe on an unauthenticated
        // socket, and — measured 2026-10-01, 7 pings 7 pongs — it still answers a
        // ping on one. So either frame makes `trackLastReceived` set
        // `isInitialized` and keep `lastReceived` fresh, which means
        // `checkLastReceived`'s 20s staleness threshold can never trip,
        // `reconnect()` never fires, and a socket holding ZERO topics reports
        // healthy to `store.socketHealth` forever. That is the original bug's
        // failure mode, not a recovery from it.
        //
        // Reconnect explicitly instead: that is a BOUNDED automatic retry, which is
        // the honest justification. It does NOT make health honest — a private
        // socket that never initialized is filtered out of `base.ts`'s
        // `initializedPrivateSockets`, and `privateHealthy` short-circuits true on
        // an empty list, so this reads connected either way. The retries are paced and
        // bounded by `processClose`'s close-code backoff (2-30s, then a 120s pause
        // after 8 consecutive failures), and in hosted mode the signer's own
        // MAX_CONSECUTIVE_FAILURES (10) force-disposes the account before the
        // socket's `> 100` reconnect cap is ever reached.
        //
        // Tell the host chain this silence was OURS. A connect that exchanged no
        // frames otherwise counts as a silent failure, and two of those advance
        // `WsDirectFallback` onto the relay and then the athena passthrough for the
        // socket's lifetime — putting every signer-less user on one shared egress
        // IP against bybit's per-IP cap, which that class exists to avoid.
        this.directFallback.disregardSilence();
        void this.reconnect();
        return;
      }

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
        const orders = this.rowsThisConnectionTrades(json.data);
        if (orders.length) this.handleOrderTopic(orders);
      }

      if (json?.topic === 'position') {
        this.recordMessageHandlerStats('position');
        const positions = this.rowsThisConnectionTrades(json.data);
        if (positions.length) this.handlePositionTopic(positions);
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

  /** Inverse and option rows arrive on this socket whatever the account turned on; see `tradesCategoryOf`. */
  private rowsThisConnectionTrades = (data: unknown): Array<Record<string, any>> =>
    Array.isArray(data) ? data.filter((row) => this.parent.tradesCategoryOf(String(row?.symbol ?? ''))) : [];

  handleOrderTopic = (data: Array<Record<string, any>>) => {
    data.forEach((order: Record<string, any>) => {
      const orders = this.parent.mapOrder(order);

      // Any WS frame for an order is its first sighting — from here on the
      // normal WS/REST reconciliation owns it and the recently-placed
      // snapshot-preserve grace must stop protecting it.
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
        const cumulativeValue = parseFloat(v(order, 'cumExecValue'));
        const fill = this.incrementalFills(
          orders[0].id,
          executed.amount,
          Number.isFinite(cumulativeValue) ? cumulativeValue : undefined,
        );
        if (fill.qty > 0) {
          this.emitter.emit('fill', {
            id: orders[0].id,
            side: orders[0].side,
            symbol: orders[0].symbol,
            // This execution's own price where the traded value allows; else the order's average.
            price: fill.value ? cleanFloat(fill.value / fill.qty) : executed.price,
            amount: fill.qty,
          });
        }
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

  private auth = async (): Promise<boolean> => {
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
        // NOT "auth failed": `health/errorPatterns.ts` lists that literal under
        // `auth_invalid`, which red-toasts "API key rejected" and makes
        // `noteTerminalExchangeFailure` suppress automatic account reloads for five
        // minutes — the very reload that would re-pair the browser signer.
        this.emitAndLogError(`Remote WS signing unavailable: ${result.error}`);
        return false;
      }

      this.sendWithLog(
        JSON.stringify({
          op: 'auth',
          args: [this.parent.options.key, expires.toFixed(0), result.wsAuthSignature],
        }),
      );
      return true;
    }

    const signature = createHmac('sha256', this.parent.options.secret).update(`GET/realtime${expires}`).digest('hex');

    const payload = {
      op: 'auth',
      args: [this.parent.options.key, expires.toFixed(0), signature],
    };

    this.sendWithLog(JSON.stringify(payload));
    return true;
  };

  private subscribe = () => {
    const payload = { op: 'subscribe', args: ['order', 'position'] };
    this.sendWithLog(JSON.stringify(payload));
  };
}
