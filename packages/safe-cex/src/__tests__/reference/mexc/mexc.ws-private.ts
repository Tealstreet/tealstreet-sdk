import type { MexcExchange } from './mexc.exchange';

import { nanoid } from 'nanoid';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity, Order, OrderStatus } from '../../../types';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { signMexcWsLogin } from './mexc.api';
import { BASE_WS_URL, isMexcControlChannel, MEXC_WS_HEARTBEAT_INTERVAL_MS } from './mexc.types';
import {
  fromMexcSymbol,
  normalizeExecution,
  normalizeOrder,
  normalizePlanOrder,
  normalizePosition,
  normalizeStopOrder,
  toIntegerString,
  toNumber,
} from './mexc.utils';

export class MexcPrivateWebsocket extends BaseWebSocket<MexcExchange> {
  protected override treatAnyMessageAsPong = true;

  private refreshBalanceTimeout?: NodeJS.Timeout;
  private heartbeatInterval?: NodeJS.Timeout;
  private readonly executionIds = new Map<string, number>();

  connectAndSubscribe = () => {
    if (this.isDisposed || this.parent.isPublicDataOnly) return;
    if (!this.parent.options.key) return;

    const key = this.parent.options.testnet ? 'testnet' : 'livenet';
    this.ws = this.createWebsocket(BASE_WS_URL.private[key], undefined, { headers: { 'User-Agent': 'Tealstreet' } });
    this.ws.addEventListener('open', this.onOpen);
    this.ws.addEventListener('message', this.onMessage);
    this.ws.addEventListener('close', this.handleClose);
  };

  onOpen = () => {
    if (this.isDisposed) return;
    this.startHeartbeat();
    this.sendLogin();
    this.parent.fetchAndSetPrivateSnapshot(10);
    this.emitWsReconnected();
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithDiagnostics(JSON.stringify({ method: 'ping' }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();

    const json = bigIntAsStringJsonParse<Record<string, any>>(String(data));
    if (!json) return;
    this.emitReceiveDiagnostics(data);

    if (json.channel === 'pong' || json.msg === 'pong' || json.method === 'pong') {
      this.handlePongEvent(15_000);
      return;
    }

    if (json.channel === 'rs.login') {
      if (json.data === 'success' || json.success === true) {
        this.subscribePrivateFeeds();
        this.emitReceiveDiagnostics('[mexc login] success');
      } else {
        this.emitReceiveDiagnostics(`[mexc login] failed ${JSON.stringify(json)}`);
        this.parent.emitAndLogError('MEXC private ws login failed, double check your key and secret');
      }
      return;
    }

    if (json.success === false) {
      this.parent.log(
        `MEXC private WS request failed: ${json.message || json.msg || 'unknown error'}`,
        LogSeverity.Warning,
      );
      return;
    }

    const channel = String(json.channel || '');
    if (!channel) return;
    if (!isMexcControlChannel(channel)) {
      this.recordMessageHandlerStats(channel);
    }

    switch (channel) {
      case 'push.personal.order':
        this.handleOrder(json);
        break;
      case 'push.personal.order.deal':
        this.handleFill(json);
        break;
      case 'push.personal.position':
        this.handlePosition(json);
        break;
      case 'push.personal.asset':
        this.throttledFetchBalance();
        break;
      // MEXC futures also emit plan and stop order lifecycle frames on the
      // same private WS. Without subscribing to and handling them here, the
      // TP/SL half of the order surface would be REST-poll only and users
      // would see chart lines flicker during rapid drag/place bursts while
      // `planorder/list` / `stoporder/list` catch up. See MEXC docs:
      // "All private data will be pushed after login: order, order.deal,
      // position, plan.order, stop.order, stop.planorder, ..." — the
      // matching push channels follow the `push.personal.<filter>` pattern.
      case 'push.personal.plan.order':
      case 'push.personal.trigger.order':
        this.handlePlanOrder(json);
        break;
      case 'push.personal.stop.order':
      case 'push.personal.stop.planorder':
        this.handleStopOrder(json);
        break;
    }
  };

  dispose = () => {
    this.stopHeartbeat();
    clearTimeout(this.refreshBalanceTimeout);
    this.executionIds.clear();
    super.dispose();
  };

  private startHeartbeat = () => {
    this.stopHeartbeat();
    this.ping();
    this.heartbeatInterval = setInterval(() => {
      if (!this.isDisposed && this.isConnected) {
        this.ping();
      }
    }, MEXC_WS_HEARTBEAT_INTERVAL_MS);
  };

  private stopHeartbeat = () => {
    clearInterval(this.heartbeatInterval);
    this.heartbeatInterval = undefined;
  };

  private handleClose = (event?: any) => {
    this.stopHeartbeat();
    this.onClose(event);
  };

  private sendLogin = async () => {
    const apiKey = this.parent.options.key.trim();
    const reqTime = `${virtualClock.getCurrentTimeMs()}`;
    const signature = await this.createLoginSignature(apiKey, reqTime);
    if (!signature) return;

    this.sendWithDiagnostics(
      JSON.stringify({
        method: 'login',
        param: {
          apiKey,
          reqTime,
          signature,
        },
      }),
      { redactAuth: true },
    );
  };

  private createLoginSignature = async (apiKey: string, reqTime: string): Promise<string | undefined> => {
    const remoteSigner = this.parent.options.remoteSigner;
    if (!remoteSigner) {
      return signMexcWsLogin(this.parent.options.secret || '', apiKey, reqTime);
    }

    const result = await remoteSigner.sign({
      id: nanoid(),
      type: 'ws-auth',
      exchangeName: 'mexc',
      accountName: remoteSigner.accountName,
      wsAuth: {
        toSign: `${apiKey}${reqTime}`,
        outputFormat: 'hex',
        extraData: { reqTime },
      },
    });

    if (result.error || !result.wsAuthSignature) {
      // NOT "auth failed": `health/errorPatterns.ts` matches that literal under
      // `auth_invalid`, which red-toasts "API key rejected" and makes
      // `noteTerminalExchangeFailure` suppress automatic account reloads for five
      // minutes — the very reload that would re-pair the browser signer. A
      // backgrounded tab or a 15s signer timeout must not look like a bad key.
      // Mirrors the bybit fix in `bybit/bybit.ws-private.ts` → auth.
      this.parent.emitAndLogError(`Remote WS signing unavailable: ${result.error ?? 'no signature returned'}`);
      return undefined;
    }
    return result.wsAuthSignature;
  };

  private subscribePrivateFeeds = () => {
    // MEXC's `personal.filter` is a *narrowing* filter, not a subscription:
    // by default the login pushes every private topic, and sending an
    // explicit filter list excludes anything not named here. If plan/stop
    // filters are omitted, the venue silently drops those pushes and
    // TP/SL sync degrades to REST-polling only. See mexc.exchange CLAUDE.md.
    this.sendWithDiagnostics(
      JSON.stringify({
        method: 'personal.filter',
        param: {
          filters: [
            { filter: 'order' },
            { filter: 'order.deal' },
            { filter: 'position' },
            { filter: 'asset' },
            { filter: 'plan.order' },
            { filter: 'stop.order' },
            { filter: 'stop.planorder' },
          ],
        },
      }),
    );
  };

  private sendWithDiagnostics = (data: string, options?: { redactAuth?: boolean }) => {
    this.send(data);
    this.emitWsDiagnosticSend(data, {
      format: (message) => `[mexc private] ${options?.redactAuth ? this.redactAuthFrame(message) : message}`,
    });
  };

  private emitReceiveDiagnostics = (data: unknown) => {
    this.emitWsDiagnosticReceive(data, {
      format: (message) => `[mexc private] ${String(message)}`,
    });
  };

  private redactAuthFrame = (data: string): string => {
    try {
      const payload = JSON.parse(data);
      if (payload?.method === 'login' && payload.param) {
        payload.param = {
          ...payload.param,
          apiKey: '[redacted]',
          signature: '[redacted]',
        };
      }
      return JSON.stringify(payload);
    } catch {
      return '[mexc private login redacted]';
    }
  };

  private handleOrder = (json: Record<string, any>) => {
    const data = json.data || json;
    const rows = Array.isArray(data) ? data : [data];
    const latest = new Map<string, Order>();
    const orders = rows
      .map((row) => normalizeOrder(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]))
      .filter((order): order is Order => !!order)
      .map((order) => {
        const next = this.parent.mergeOrderSnapshot(order, latest.get(order.id));
        latest.set(order.id, next);
        return next;
      });

    if (orders.length) {
      this.store.addOrUpdateOrders(orders as any);
      for (const order of orders) {
        if (order.status === OrderStatus.Closed || order.status === OrderStatus.Canceled) {
          this.store.markOrdersTerminal([{ id: order.id }], order.status);
        }
      }
    }

    // Detect MEXC-side rejections. A `state=4` (Canceled) frame carrying a
    // non-zero `errorCode` with `dealVol=0` means MEXC accepted the order
    // and then immediately canceled it server-side. The most common trigger
    // is a reduce-only order whose total open close size would exceed the
    // position — MEXC emits `state=2` (Open) then `state=4` within ~1ms.
    // The HTTP 200 from `POST /order/submit` typically arrives AFTER both
    // WS frames, so the placement site's optimistic-add path and the tick
    // preservation window would otherwise resurrect the row locally.
    // Register the id in the parent's rejection shield and surface a
    // user-visible error explaining why the order didn't stick.
    for (const row of rows) {
      const state = toNumber(row?.state ?? row?.status, Number.NaN);
      const errorCode = toNumber(row?.errorCode, 0);
      const dealVol = toNumber(row?.dealVol, 0);
      if (state === 4 && errorCode > 0 && dealVol === 0) {
        const orderId = toIntegerString(row?.orderId ?? row?.id);
        if (orderId) this.parent.noteRejectedRegularOrder(orderId, errorCode);
      }
    }

    // Intentionally no REST reconciliation here. MEXC's `push.personal.order`
    // payload is complete (vol/remainVol/state/price/leverage/timestamps/...),
    // so trusting the WS write plus the periodic `tick` (~2s) and
    // `scheduleUserActivityPositionRefetch` covers reconciliation without the
    // duplicate `open_orders` roundtrip that used to fire on every WS event.
  };

  private handleFill = (json: Record<string, any>) => {
    const data = json.data || json;
    const rows = Array.isArray(data) ? data : [data];
    const now = virtualClock.getCurrentTimeMs();
    for (const [id, seenAt] of this.executionIds) {
      if (now - seenAt < 300_000) break;
      this.executionIds.delete(id);
    }
    for (const row of rows) {
      const fill = normalizeExecution(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]);
      if (!fill) continue;
      if (fill.id) {
        const duplicate = this.executionIds.has(fill.id);
        this.executionIds.delete(fill.id);
        this.executionIds.set(fill.id, now);
        if (duplicate) continue;
      }
      this.parent.emitter.emit('fill', {
        id: fill.id || fill.orderId || `${fill.symbol}:${fill.timestamp}`,
        orderId: fill.orderId,
        symbol: fill.symbol,
        side: fill.side,
        price: fill.price,
        amount: fill.qty,
        fee: fill.commission,
        realizedPnl: fill.realizedPnl,
      });
    }
    // MEXC emits a separate `push.personal.order` (order state) and
    // `push.personal.position` (position delta) alongside every fill, so
    // there is no need to fan out to REST from the deal handler.
  };

  private handlePosition = (json: Record<string, any>) => {
    const data = json.data || json;
    const rows = Array.isArray(data) ? data : [data];
    const positions = rows
      .map((row) => normalizePosition(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]))
      .filter(Boolean);
    if (positions.length) {
      this.store.addOrUpdatePositions(positions as any);
    }
    // Intentionally no REST reconciliation here. `push.personal.position`
    // is a full snapshot (holdVol/frozenVol/holdAvgPrice/leverage/liqPrice/
    // marginRatio/pnl/...) so the WS write is enough; the 2s `tick` acts as
    // the safety net.
  };

  // MEXC standalone plan orders (untriggered TP/SL and other trigger orders
  // placed via `POST /planorder/place/v2`). These arrive on
  // `push.personal.plan.order` (and, per CCXT, `push.personal.trigger.order`).
  // The frame carries the same core fields as the REST `planorder/list`
  // response — `id`/`symbol`/`side`/`triggerPrice`/`vol`/`state`/`triggerType`
  // — so `normalizePlanOrder` can consume it directly. Any id observed on the
  // WS is marked seen so the recently-placed-stop shield stops protecting it;
  // from that point on, normal WS/REST reconciliation owns the row.
  private handlePlanOrder = (json: Record<string, any>) => {
    this.applyStopLikeFrame(json, 'plan');
  };

  // MEXC position TP/SL (`stoporder/place` / `stoporder/change_price`) and
  // stop-limit plan trigger orders (`stoporder/change_plan_price`). Both come
  // through the `stop.order` / `stop.planorder` filters and land on
  // `push.personal.stop.order` / `push.personal.stop.planorder`. Frame shape
  // follows the REST `stoporder/list` response — `id` (or `stopPlanOrderId`),
  // `positionType`, `stopLossPrice`/`takeProfitPrice`, `state`, `volType`,
  // `triggerSide` — which `normalizeStopOrder` already accepts.
  private handleStopOrder = (json: Record<string, any>) => {
    this.applyStopLikeFrame(json, 'stop');
  };

  private applyStopLikeFrame = (json: Record<string, any>, kind: 'plan' | 'stop') => {
    const data = json.data || json;
    const rows = Array.isArray(data) ? data : [data];
    const normalize = kind === 'plan' ? normalizePlanOrder : normalizeStopOrder;
    const parsed = rows
      .map((row) => normalize(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]))
      .filter((order): order is Order => !!order);

    // Mark every id we observed on the wire as "seen" so the placement
    // shield stops protecting it. This runs even for stale frames — the
    // placement shield only cares whether the venue knows the id exists.
    for (const row of rows) {
      const id = toIntegerString(row?.id ?? row?.stopPlanOrderId ?? row?.orderId);
      if (id) this.parent.markStopLikeOrderSeen(id);
    }

    // The venue occasionally flushes a queued `state=1` (Open) snapshot for
    // an order the user just canceled locally, arriving *after* our HTTP
    // cancel completed. `addOrUpdateOrders` overwrites the whole row, so
    // without this guard the locally-canceled row flips back to Open and
    // the chart line reappears until either the real `state=2` frame or
    // the next REST tick shakes it out again — visible as flicker /
    // "sticky" TP/SL / reappearing lines during rapid cancel bursts.
    // Canceled and Closed frames are always honored (they're authoritative
    // regardless of what our local shield thinks).
    const latest = new Map<string, Order>();
    const orders = parsed.filter((order) => {
      if (order.status !== OrderStatus.Open) return true;
      if (this.parent.isRecentlyCanceledStopLikeOrderId(order.id)) return false;
      return true;
    }).map((order) => {
      const next = this.parent.mergeOrderSnapshot(order, latest.get(order.id));
      latest.set(order.id, next);
      return next;
    });

    if (!orders.length) return;

    // `addOrUpdateOrders` writes the full normalized row (including the
    // Canceled / Closed status carried by the frame), so a separate
    // `updateOrderStatus` call would just re-emit the store notification
    // for the same terminal transition — skip it.
    this.store.addOrUpdateOrders(orders as any);
    // Intentionally no REST fan-out. The plan/stop frame is a full-row
    // snapshot; the 1.5s coalesced `scheduleStopOrderSnapshotRefresh` fires
    // after user actions and covers cases where a frame is missed or dropped.
  };

  private throttledFetchBalance = () => {
    clearTimeout(this.refreshBalanceTimeout);
    this.refreshBalanceTimeout = setTimeout(async () => {
      try {
        const balance = await this.parent.fetchBalance();
        if (!this.isDisposed) this.store.update({ balance });
      } catch (_err) {}
    }, 1000);
  };
}
