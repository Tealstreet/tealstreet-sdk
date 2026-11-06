import type { HyrotraderExchange } from './hyrotrader.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { virtualClock } from '../../../utils/virtualClock';
import { BybitPrivateWebsocket } from './bybit.ws-private';

type PendingAck = {
  resolve: () => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
  op: string;
};

export class HyroPrivateWebsocket extends BybitPrivateWebsocket {
  // Its session handshake gates every event; online recovery falls back to an account reload.
  protected readonly recoversAfterOffline = false;

  declare parent: HyrotraderExchange;
  ready = false;
  started = false;
  stateRevision = 0;
  private generation = 0;
  private pendingAcks = new Map<string, PendingAck>();

  get sessionVersion() {
    return this.generation;
  }

  get isRecoveryComplete() {
    return !this.started || this.ready;
  }

  constructor(parent: HyrotraderExchange) {
    super(parent);
    const processOrderTopic = this.handleOrderTopic;
    this.handleOrderTopic = (rows) => {
      this.parent.observePrivateOrders(rows);
      processOrderTopic(rows);
    };
    const processMessage = this.onMessage;
    const processClose = this.onClose;
    const connect = this.connectAndSubscribe;
    const ping = this.ping;
    this.connectAndSubscribe = () => {
      this.started = true;
      this.resetSession();
      connect();
    };
    this.onClose = (event) => {
      if (event?.currentTarget && event.currentTarget !== this.ws) return;
      this.resetSession();
      processClose(event);
    };
    this.ping = () => {
      if (this.ready) ping();
    };
    this.onMessage = (event) => {
      if (this.isDisposed || (event.currentTarget && event.currentTarget !== this.ws)) return;
      const message = jsonParse(event.data);
      if (message?.op && message.req_id) {
        const pending = this.pendingAcks.get(message.req_id);
        if (pending && pending.op === message.op) {
          this.pendingAcks.delete(message.req_id);
          clearTimeout(pending.timer);
          if (message.success === true) pending.resolve();
          else
            pending.reject(
              new Error(`HyroTrader private ${message.op} rejected: ${message.ret_msg || 'request rejected'}`),
            );
        }
      }
      if (message?.topic) this.stateRevision++;
      if (this.ready) processMessage(event);
    };
  }

  private resetSession() {
    this.generation++;
    this.stateRevision++;
    this.ready = false;
    this.lastReceived = 0;
    for (const pending of this.pendingAcks.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error('HyroTrader private socket session ended'));
    }
    this.pendingAcks.clear();
    this.parent.syncSocketHealthToStore?.();
  }

  // Recovery belongs after acknowledged auth/subscription, never the base open callback.
  protected get resyncsPrivateStateOnReopen() {
    return false;
  }

  sendReconnectSequence = async () => {
    this.started = true;
    this.resetSession();
    const generation = this.generation;
    const socket = this.ws;
    const isCurrent = () => !this.isDisposed && generation === this.generation && socket === this.ws;
    try {
      const expires = virtualClock.getCurrentTimeMs() + 10_000;
      let signature: string;
      if (this.parent.options.remoteSigner) {
        const result = await this.parent.options.remoteSigner.sign({
          id: nanoid(),
          type: 'ws-auth',
          exchangeName: 'bybit',
          accountName: this.parent.options.remoteSigner.accountName,
          wsAuth: { toSign: `GET/realtime${expires}`, outputFormat: 'hex', extraData: { expires } },
        });
        if (result.error || !result.wsAuthSignature)
          throw new Error(`Remote WS signing unavailable: ${result.error || 'missing signature'}`);
        signature = result.wsAuthSignature;
      } else {
        signature = createHmac('sha256', this.parent.options.secret).update(`GET/realtime${expires}`).digest('hex');
      }
      if (!isCurrent()) return;
      await this.request('auth', [this.parent.options.key, expires, signature]);
      if (!isCurrent()) return;
      await this.request('subscribe', ['order', 'position']);
      for (let attempt = 0; attempt < 3 && isCurrent(); attempt++) {
        if (await this.parent.reconcilePrivateState(isCurrent)) {
          if (!isCurrent()) return;
          this.ready = true;
          this.trackLastReceived();
          this.ping();
          return;
        }
      }
      if (isCurrent()) throw new Error('HyroTrader private state did not stabilize during reconciliation');
    } catch (error) {
      if (!isCurrent()) return;
      this.emitAndLogError(error instanceof Error ? error : new Error(String(error)));
      this.resetSession();
      void this.reconnect();
    }
  };

  private request(op: string, args: unknown[]): Promise<void> {
    const req_id = nanoid();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingAcks.delete(req_id);
        reject(new Error(`HyroTrader private ${op} acknowledgement timed out`));
      }, 10_000);
      this.pendingAcks.set(req_id, { op, resolve, reject, timer });
      try {
        this.sendWithLog(JSON.stringify({ op, req_id, args }));
      } catch (error) {
        clearTimeout(timer);
        this.pendingAcks.delete(req_id);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  dispose() {
    this.resetSession();
    super.dispose();
  }
}
