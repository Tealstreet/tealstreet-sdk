import createHmac from 'create-hmac';
import flatten from 'lodash/flatten';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_PWS_LOG, ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { Candle, OHLCVOptions } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { BASE_WS_URLS, INTERVAL } from './okx.types';
import { OKXPrivateWebsocket } from './okx.ws-private';

type Data = Record<string, any>;
type SubscribedTopics = {
  [id: string]: Array<{ channel: string; instId: string }>;
};

export class OKXBusinessWebsocket extends OKXPrivateWebsocket {
  topics: SubscribedTopics = {};

  get endpoint() {
    return BASE_WS_URLS[this.parent.region].business[this.parent.options.testnet ? 'testnet' : 'livenet'];
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(
        this.parent.options.tealstreetProxy ? 'wss://fsjnpy143pbz.asia.tealstreet.io/okx/business' : this.endpoint,
      );
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.auth();
      this.ping();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog('ping');
    }
  };

  auth = async () => {
    const timestamp = virtualClock.getCurrentTime().unix();

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: nanoid(),
        type: 'ws-auth',
        exchangeName: 'okx',
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: `${timestamp}GET/users/self/verify`,
          outputFormat: 'base64',
          extraData: { timestamp },
        },
      });

      if (result.error) {
        this.emitAndLogError(`Remote WS auth failed: ${result.error}`);
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
            },
          ],
        }),
      );
      return;
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
  };

  subscribe = () => {
    this.sendWithLog(
      JSON.stringify({
        op: 'subscribe',
        args: [
          { channel: 'orders-algo', instType: 'SWAP' },
          { channel: 'algo-advance', instType: 'SWAP' },
        ],
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
      if (ENABLE_PWS_LOG) {
        this.parent.emitter.emit('grafana-pws', `${data}`);
      }

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

      if (
        data.includes('"channel":"orders"') ||
        data.includes('"channel":"orders-algo"') ||
        data.includes('"channel":"algo-advance"')
      ) {
        const json = jsonParse(data);
        if (json) this.handleOrderTopic(json);
        return;
      }

      for (const [channel, handler] of Object.entries(this.messageHandlers)) {
        let [leftmost, _mid, instId] = channel.split('.');
        if (leftmost === 'books') {
          instId = _mid;
        }
        if (
          data.includes(`channel":"${leftmost}`) &&
          (!instId || data.includes(`instId":"${instId}`)) &&
          !data.includes('event":"subscribe"')
        ) {
          const json = jsonParse(data);
          for (const cb of handler) {
            if (json) cb(json);
          }
        }
      }
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
