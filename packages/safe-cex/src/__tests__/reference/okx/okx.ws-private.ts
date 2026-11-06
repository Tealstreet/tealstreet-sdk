import type { OKXExchange } from './okx.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { resolveTransportRouting } from '../../../transport/registry';
import { isWsRelayEnabled } from '../../../transport/tealstreet/wsRelay';
import { OrderStatus } from '../../../types';
import { roundUSD } from '../../../utils/roundUSD';
import { virtualClock } from '../../../utils/virtualClock';
import { WsDirectFallback } from '../../../utils/wsDirectFallback';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URLS } from '../../../exchanges/okx/okx.types';

export class OKXPrivateWebsocket extends BaseWebSocket<OKXExchange> {
  private recentFillKeys = new Map<string, number>();

  protected directFallback = new WsDirectFallback();

  private readonly fillDedupeWindowMs = 60_000;

  private parseFinite = (value: unknown) => {
    const parsed = parseFloat(String(value ?? ''));
    return Number.isFinite(parsed) ? parsed : 0;
  };

  private sumFinite = (values: unknown[]): number => {
    return values.reduce<number>((sum, value) => sum + this.parseFinite(value), 0);
  };

  get endpoint() {
    return BASE_WS_URLS[this.parent.region].private[this.parent.options.testnet ? 'testnet' : 'livenet'];
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const route = resolveTransportRouting(this.parent.name, this.parent.options).wsPrivate;
      const baseUrl = this.directFallback.resolveUrl(
        route.tunnelled ? route.host! : this.endpoint,
        isWsRelayEnabled() ? route.relay : undefined,
        route.fallback,
      );
      if (baseUrl === route.fallback || baseUrl === route.relay) {
        try {
          this.parent.emitter.emit(
            'grafana-info',
            `Using tealstreet okx private ${baseUrl === route.relay ? 'relay' : 'proxy'}`,
          );
        } catch (_e) {}
      }
      this.ws = this.createWebsocket(baseUrl);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = async () => {
    if (this.isDisposed) return;
    let authed = false;
    try {
      authed = await this.auth();
    } catch (error) {
      // `remoteSigner.sign` THROWS (unavailable, timed out, aborted) rather than
      // resolving `{ error }`; un-awaited this was an unhandled rejection and a
      // socket that never logged in. Pass the error alone — see bybit.ws-private.
      this.emitAndLogError(error instanceof Error ? error : new Error(String(error)));
    }
    if (this.isDisposed) return;
    if (!authed) {
      // Send NOTHING, not even a ping: a pong on an unauthenticated socket keeps a
      // zero-topic socket looking healthy forever (measured on bybit; unmeasured
      // here, so not risked). Reconnect, and tell the host chain the silence was
      // ours. See ../CLAUDE.md → "Remote-signer WS auth".
      this.directFallback.disregardSilence();
      void this.reconnect();
      return;
    }
    this.ping();
    if (ENABLE_RECONNECTING_WEBSOCKET) {
      this.parent.fetchAndSetPrivateSnapshot();
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog('ping');
    }
  };

  auth = async (): Promise<boolean> => {
    const timestamp = virtualClock.getCurrentTime().unix();

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: nanoid(),
        type: 'ws-auth',
        exchangeName: this.parent.name,
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: `${timestamp}GET/users/self/verify`,
          outputFormat: 'base64',
          extraData: { timestamp },
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
        return false;
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
            },
          ],
        }),
      );
      return true;
    }

    const signature = createHmac('sha256', this.parent.options.secret)
      .update([timestamp, 'GET', '/users/self/verify'].join(''))
      .digest('base64');

    this.sendWithLog(
      JSON.stringify({
        op: 'login',
        args: [
          {
            apiKey: this.parent.options.key,
            passphrase: this.parent.options.passphrase,
            timestamp,
            sign: signature,
          },
        ],
      }),
    );
    return true;
  };

  subscribe = () => {
    const productTopics = this.parent.getInstrumentTypes().flatMap((instType) => [
      { channel: 'positions', instType },
      { channel: 'orders', instType },
    ]);

    this.sendWithLog(
      JSON.stringify({
        op: 'subscribe',
        args: [{ channel: 'account' }, ...productTopics],
      }),
    );
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.directFallback.onMessage();
      this.emitWsDiagnosticReceive(data);

      if (data.includes('event":"subscribe"')) {
        return;
      }

      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

      if (data.includes('login')) {
        try {
          const json = jsonParse(data);
          if (+json?.code === 0) {
            this.subscribe();
          }
        } catch {}
        return;
      }

      if (data.includes('"event":"error"')) {
        try {
          const json = jsonParse(data);
          if (json && json.msg) {
            this.parent.emitAndLogError(json.msg);
          }
        } catch {
          // Ignored
        }
        return;
      }

      if (data.includes('"channel":"orders"')) {
        const json = jsonParse(data);
        if (json && json.data) this.handleOrderTopic(json);
        return;
      }

      if (data.includes('"channel":"positions"')) {
        const json = jsonParse<{ data: Array<Record<string, any>> }>(data);
        if (json && json.data) this.handlePositionTopic(json);
        return;
      }

      if (data.includes('"channel":"account"')) {
        const json = jsonParse<{ data: Array<Record<string, any>> }>(data);
        if (json) this.handleAccountTopic(json);
      }
    }
  };

  handleOrderTopic = ({ data: okxOrders }: Record<string, any>) => {
    for (const o of okxOrders) {
      const orders = this.parent.mapOrders([o]);

      if (orders.length) {
        if (o.state === 'filled' || o.state === 'canceled' || o.state === 'mmp_canceled') {
          // OKX's filled and (mmp_)canceled are final; a snapshot read before this must not reopen it.
          this.store.markOrdersTerminal(orders, o.state === 'filled' ? OrderStatus.Closed : OrderStatus.Canceled);
        }

        if (o.state === 'live' || o.state === 'partially_filled') {
          this.store.addOrUpdateOrders(orders);
        }

        if (o.state === 'filled' || o.state === 'partially_filled') {
          // Use O(1) hashmap lookup instead of O(n) find
          const market = this.store.marketsById[o.instId];
          const fillAmount = parseFloat(o.fillSz || '0');
          const fillPrice = parseFloat(o.fillPx || '0');

          // OKX can send repeated order updates for the same execution.
          // Emit only non-zero fills and dedupe by trade/order key.
          if (market && fillAmount > 0 && !this.isDuplicateFillEvent(o)) {
            this.emitter.emit('fill', {
              id: orders[0].id,
              side: orders[0].side,
              symbol: orders[0].symbol,
              price: fillPrice,
              amount: this.parent.contractsToBase(fillAmount, market, fillPrice),
            });
          }
        }
      }
    }
  };

  private isDuplicateFillEvent = (o: Record<string, any>) => {
    const now = Date.now();
    const cutoff = now - this.fillDedupeWindowMs;

    for (const [key, seenAt] of this.recentFillKeys) {
      if (seenAt < cutoff) {
        this.recentFillKeys.delete(key);
      }
    }

    const tradeId = String(o.tradeId || '');
    const fillTime = String(o.fillTime || o.uTime || '');
    const fillPx = String(o.fillPx || '');
    const fillSz = String(o.fillSz || '');
    const dedupeKey =
      tradeId && tradeId !== '0'
        ? `${o.instId}:${o.ordId}:${tradeId}`
        : `${o.instId}:${o.ordId}:${fillTime}:${fillPx}:${fillSz}`;

    if (this.recentFillKeys.has(dedupeKey)) {
      return true;
    }

    this.recentFillKeys.set(dedupeKey, now);
    return false;
  };

  /*
  2024-08-13T16:11:14.262Z
WS MESSAGE

{
  "arg": {
    "channel": "positions",
    "instType": "SWAP",
    "uid": "430670847607508964"
  },
  "data": [
    {
      "adl": "1",
      "availPos": "5",
      "avgPx": "1.0114",
      "baseBal": "",
      "baseBorrowed": "",
      "baseInterest": "",
      "bePx": "1.0203742586727123",
      "bizRefId": "",
      "bizRefType": "",
      "cTime": "1715703057468",
      "ccy": "USDT",
      "clSpotInUseAmt": "",
      "closeOrderAlgo": [],
      "deltaBS": "",
      "deltaPA": "",
      "fee": "-0.0010114",
      "fundingFee": "0.0484336290102426",
      "gammaBS": "",
      "gammaPA": "",
      "idxPx": "0.5862",
      "imr": "0.05863",
      "instId": "SUSHI-USDT-SWAP",
      "instType": "SWAP",
      "interest": "",
      "last": "0.5863",
      "lever": "50",
      "liab": "",
      "liabCcy": "",
      "liqPenalty": "0",
      "liqPx": "10.471236019180255",
      "margin": "",
      "markPx": "0.5863",
      "maxSpotInUseAmt": "",
      "mgnMode": "cross",
      "mgnRatio": "2426.4113133974597",
      "mmr": "0.01905475",
      "notionalUsd": "2.932995065",
      "optVal": "",
      "pTime": "1723565474058",
      "pendingCloseOrdLiabVal": "",
      "pnl": "0",
      "pos": "5",
      "posCcy": "",
      "posId": "710645362062995460",
      "posSide": "short",
      "quoteBal": "",
      "quoteBorrowed": "",
      "quoteInterest": "",
      "realizedPnl": "0.0474222290102426",
      "spotInUseAmt": "",
      "spotInUseCcy": "",
      "thetaBS": "",
      "thetaPA": "",
      "tradeId": "133489546",
      "uTime": "1723564800784",
      "upl": "2.1255",
      "uplLastPx": "2.1255",
      "uplRatio": "21.015424164524422",
      "uplRatioLastPx": "21.015424164524422",
      "usdPx": "",
      "vegaBS": "",
      "vegaPA": ""
    }
  ]
}
   */

  handlePositionTopic = ({ data: okxPositions }: { data: Array<Record<string, any>> }) => {
    const positions = this.parent.mapPositions(okxPositions);

    if (positions.length) {
      const used = roundUSD(
        this.sumFinite(
          okxPositions.map((p) => {
            const market = this.store.marketsById[p.instId];
            return this.parent.convertMarginValueToUsd(
              this.parseFinite(p.mmr),
              p.ccy,
              market,
              this.parseFinite(p.markPx),
            );
          }),
        ),
      );
      const upnl = roundUSD(this.sumFinite(positions.map((position) => position.unrealizedPnl)));

      // Below is fixed by handling position id with correct mode
      // in one way mode position id is only created with symbol

      // // OKX doesn't sends position side for net positions,
      // // so when we are closing a position and contracts become 0,
      // // we can't tell if it was from the long or short side.
      // //
      // // To fix this, we keep in store "virtual positions" for both sides,
      // // and when we receive a position update with 0 contracts, we update
      // // both virtual positions.
      // const updates: Array<[Pick<Position, 'id'>, Partial<Position>]> = positions.flatMap((p) =>
      //   p.contracts === 0 && this.parent._accountPositionMode !== PositionMode.Hedged
      //     ? [
      //         [{ id: createPositionId(p.symbol, PositionSide.Long) }, { ...p, side: PositionSide.Long }],
      //         [{ id: createPositionId(p.symbol, PositionSide.Short) }, { ...p, side: PositionSide.Short }],
      //       ]
      //     : [[{ id: createPositionId(p.symbol, p.side) }, p]],
      // );

      this.store.addOrUpdatePositions(positions);
      this.store.update({
        balance: { ...this.store.balance, used, upnl },
      });
    }
  };

  handleAccountTopic = ({ data }: { data?: Array<Record<string, any>> }) => {
    this.parent.applyAccountEquity(data);
  };
}
