import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { BaseWebSocket } from '../base.ws';
import type { BreakoutpropExchange } from './breakoutprop.exchange';
import { BASE_WS_URL } from './breakoutprop.types';

export class BitruePrivateWebsocket extends BaseWebSocket<BreakoutpropExchange> {
  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(BASE_WS_URL.private + '?requestId=' + nanoid());

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.auth();
      this.subscribe();
      this.ping();
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      const _json = jsonParse(data);
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ op: 'ping' }));
    }
  };

  private auth = () => {
    const expires = new Date().getTime() + 10_000;
    const signature = createHmac('sha256', this.parent.options.secret).update(`GET/realtime${expires}`).digest('hex');

    const payload = {
      op: 'auth',
      args: [this.parent.options.key, expires.toFixed(0), signature],
    };

    this.send(JSON.stringify(payload));
  };

  private subscribe = () => {
    const payload = { op: 'subscribe', args: ['order', 'position'] };
    this.send(JSON.stringify(payload));
  };
}
