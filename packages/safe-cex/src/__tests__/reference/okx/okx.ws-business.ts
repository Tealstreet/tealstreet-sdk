import type { Candle, OHLCVOptions, Timeframe } from '../../../types';

import createHmac from 'create-hmac';
import flatten from 'lodash/flatten';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { resolveTransportRouting } from '../../../transport/registry';
import { isWsRelayEnabled } from '../../../transport/tealstreet/wsRelay';
import { virtualClock } from '../../../utils/virtualClock';
import { BASE_WS_URLS, INTERVAL } from '../../../exchanges/okx/okx.types';
import { OKXPrivateWebsocket } from './okx.ws-private';

type Data = Record<string, any>;
type SubscribedTopics = {
  [id: string]: Array<{ channel: string; instId: string }>;
};

const TIMEFRAME_BY_CANDLE_CHANNEL = Object.fromEntries(
  Object.entries(INTERVAL).map(([timeframe, interval]) => [`candle${interval}`, timeframe]),
) as Record<string, Timeframe>;

export const ORDER_CHANNELS = new Set(['orders', 'orders-algo', 'algo-advance']);
// Single stats topic for all three; see the record call below. Exported so the
// routing regression test asserts the real key instead of a literal that can
// drift away from it.
export const ORDER_STATS_TOPIC = 'orders';

export class OKXBusinessWebsocket extends OKXPrivateWebsocket {
  topics: SubscribedTopics = {};

  // Carries algo orders, so a re-open resyncs private state like the private socket.
  protected get resyncsPrivateStateOnReopen() {
    return true;
  }

  get endpoint() {
    return BASE_WS_URLS[this.parent.region].business[this.parent.options.testnet ? 'testnet' : 'livenet'];
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const route = resolveTransportRouting(this.parent.name, this.parent.options).wsBusiness;
      const baseUrl = this.directFallback.resolveUrl(
        route.tunnelled ? route.host! : this.endpoint,
        isWsRelayEnabled() ? route.relay : undefined,
        route.fallback,
      );
      if (baseUrl === route.fallback || baseUrl === route.relay) {
        try {
          this.parent.emitter.emit(
            'grafana-info',
            `Using tealstreet okx business ${baseUrl === route.relay ? 'relay' : 'proxy'}`,
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
    if (this.parent.isPublicDataOnly) {
      this.subscribe();
      this.ping();
      return;
    }
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
      this.emitWsReconnected();
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
    const orderTopics = this.parent.getInstrumentTypes().flatMap((instType) => [
      { channel: 'orders-algo', instType },
      { channel: 'algo-advance', instType },
    ]);

    if (!this.parent.isPublicDataOnly)
      this.sendWithLog(
        JSON.stringify({
          op: 'subscribe',
          args: orderTopics,
        }),
      );

    const topics = flatten(Object.values(this.topics));
    const batchSize = 100;
    for (let i = 0; i < topics.length; i += batchSize) {
      const batch = topics.slice(i, i + batchSize);
      this.sendWithLog(JSON.stringify({ op: 'subscribe', args: batch }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.directFallback.onMessage();
      this.emitWsDiagnosticReceive(data);

      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

      const json = jsonParse<Data>(data);
      if (!json || json.event === 'subscribe') return;

      if (json.event === 'login') {
        if (+json.code === 0) {
          this.subscribe();
        }
        return;
      }

      const channel = typeof json.arg?.channel === 'string' ? json.arg.channel : '';
      const instId = typeof json.arg?.instId === 'string' ? json.arg.instId : '';
      const instType = typeof json.arg?.instType === 'string' ? json.arg.instType : '';
      if (!channel) return;

      if (ORDER_CHANNELS.has(channel)) {
        // All three collapse to one stats topic. OKX pushes an order channel
        // only for a category the account currently holds orders in, so tracking
        // them separately produced up to three entries that each went quiet
        // independently as categories emptied. One entry, kept fresh by any
        // order traffic, matches how the client gates it: on whether the account
        // holds any open orders at all.
        this.recordMessageHandlerStats(ORDER_STATS_TOPIC);
        this.handleOrderTopic(json);
        return;
      }

      const timeframe = TIMEFRAME_BY_CANDLE_CHANNEL[channel];
      if (timeframe && instId) {
        this.dispatchMessageHandler(`candle.${timeframe}.${instId}`, json);
      }

      if (instType && instId) {
        this.dispatchMessageHandler(`${channel}.${instType}.${instId}`, json);
      }
      if (instId) {
        this.dispatchMessageHandler(`${channel}.${instId}`, json);
      }
      this.dispatchMessageHandler(channel, json);
    }
  };

  private dispatchMessageHandler = (handler: string, json: Data) => {
    const callbacks = this.messageHandlers[handler];
    if (!callbacks?.length) return;

    this.recordMessageHandlerStats(handler);
    for (const callback of callbacks) {
      callback(json);
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    if (!this.store.loaded.markets) {
      timeoutId = setTimeout(() => this.listenOHLCV(opts, callback), 100);

      return () => {
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
      };
    }

    const market = this.store.markets.find((m) => m.symbol === opts.symbol);
    if (!market) return () => {};

    const handler = `candle.${opts.interval}.${market.id}`;

    const topic = {
      channel: `candle${INTERVAL[opts.interval]}`,
      instId: market.id,
    };

    const parser = (data: Data) => {
      const candle = data?.data?.[0];

      if (candle) {
        callback({
          timestamp: parseInt(candle[0], 10) / 1000,
          open: parseFloat(candle[1]),
          high: parseFloat(candle[2]),
          low: parseFloat(candle[3]),
          close: parseFloat(candle[4]),
          volume: parseFloat(candle[7]),
        });
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

          this.send(JSON.stringify({ op: 'subscribe', args: [topic] }));
          //this.parent.log(`Switched to [${opts.symbol}:${opts.interval}]`);

          // store the topic so we can unsubscribe later
          this.topics[handler] = [topic];
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];

      if (!this.messageHandlers[handler].length) {
        delete this.topics[handler];
        this.cleanupMessageHandlerStats(handler);
      }

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = { op: 'unsubscribe', args: [topic] };
        this.send(JSON.stringify(payload));
      }
    };
  };
}
