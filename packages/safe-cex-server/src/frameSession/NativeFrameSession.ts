import type { AccountAuthResult } from '@tealstreet/safe-cex-wasm/host';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';

/** The standard WebSocket surface; no Node-specific socket implementation is required. */
export interface NativeFrameSocket {
  binaryType: BinaryType;
  readonly bufferedAmount: number;
  readonly readyState: number;
  readonly protocol: string;
  addEventListener(type: string, listener: EventListener): void;
  removeEventListener(type: string, listener: EventListener): void;
  send(data: string): void;
  close(code?: number, reason?: string): void;
}

export type NativeFrameSocketFactory = (url: string, protocols: string[]) => NativeFrameSocket;

export interface NativeFrameSessionSink {
  /** Delivery is serial, and commands remain gated until the snapshot delivery completes. */
  onFrame(buffer: ArrayBuffer, metadata: { snapshot: boolean }): void | Promise<void>;
  /** Native notices, including accountAuth, retaining their original JSON. */
  onNotice(rawJson: string): void | Promise<void>;
  /** Optional typed account evidence, delivered before its raw onNotice callback.
   * Scoped to this connection; it never belongs to a pending command. */
  onAccountAuth?(result: AccountAuthResult): void | Promise<void>;
  /** Called once, including explicit close. The error never contains connection credentials. */
  onClose(error: Error): void;
}

export interface NativeFrameSessionOptions {
  /** Trusted, resolved account endpoint: ws[s]://host/accounts/<account-id>. */
  url: string;
  token: string;
  sink: NativeFrameSessionSink;
  socketFactory?: NativeFrameSocketFactory;
  /** Ends pending snapshot delivery and the established session; no reconnect. */
  signal?: AbortSignal;
  connectTimeoutMs?: number;
  /** Queue accounting includes the currently executing sink callback. */
  maxQueuedMessages?: number;
  maxQueuedBytes?: number;
  maxFrameBytes?: number;
  maxCommandBytes?: number;
  maxBufferedAmount?: number;
}

type Limits = Required<Pick<NativeFrameSessionOptions, 'connectTimeoutMs' | 'maxQueuedMessages' | 'maxQueuedBytes' | 'maxFrameBytes' | 'maxCommandBytes' | 'maxBufferedAmount'>>;
type Delivery = { bytes: number } & ({ buffer: ArrayBuffer; snapshot: boolean } | { text: string; auth?: AccountAuthResult });

const PROTOCOL = 'safe-cex-frame.v1';
// Opts in to accountAuth notices; servers that predate it ignore the offer.
const ACCOUNT_AUTH_PROTOCOL = 'safe-cex-frame.account-auth';
const encoder = new TextEncoder();
const error = () => new Error('Native frame session closed');
const invalid = () => new Error('Invalid native frame session configuration');
const uint32 = (value: unknown) => Number.isInteger(value) && Number(value) >= 0 && Number(value) <= 0xffffffff;
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const keys = (value: Record<string, unknown>, allowed: string[]) => Object.keys(value).every((key) => allowed.includes(key));

function validateNotice(text: string): AccountAuthResult | undefined {
  const value: unknown = JSON.parse(text);
  if (!record(value)) throw error();
  switch (value.type) {
    case 'accountAuth':
      if (keys(value, ['type', 'credential', 'scope', 'outcome'])
        && (value.credential === 'primary' || value.credential === 'secondary')
        && typeof value.scope === 'string' && ['account', 'balance', 'orders', 'positions', 'settings'].includes(value.scope)
        && (value.outcome === 'accepted' || value.outcome === 'rejected')) {
        return Object.freeze({ credential: value.credential, scope: value.scope as AccountAuthResult['scope'], outcome: value.outcome });
      }
      break;
    case 'commandError':
      if (keys(value, ['type', 'command', 'message']) && uint32(value.command) && typeof value.message === 'string') return;
      break;
    case 'commandProgress':
      if (keys(value, ['type', 'command', 'receipts']) && uint32(value.command) && Array.isArray(value.receipts) && value.receipts.every((receipt) =>
        record(receipt) && keys(receipt, ['symbol', 'kind', 'identity']) && typeof receipt.symbol === 'string' && typeof receipt.identity === 'string' &&
        typeof receipt.kind === 'string' && ['clientOrderId', 'clientOrderIndex', 'orderCode'].includes(receipt.kind),
      )) return;
      break;
    case 'done':
      if (keys(value, ['type', 'command', 'result', 'error', 'detail']) && uint32(value.command) && ((typeof value.result === 'string' && !('error' in value) && !('detail' in value)) || (typeof value.error === 'string' && !('result' in value) && (!('detail' in value) || record(value.detail))))) return;
      break;
    case 'error':
      if (keys(value, ['type', 'command', 'error']) && typeof value.error === 'string' && (!('command' in value) || uint32(value.command))) return;
      break;
    case 'log':
      if (keys(value, ['type', 'level', 'message']) && typeof value.level === 'string' && ['info', 'warning', 'error'].includes(value.level) && typeof value.message === 'string') return;
      break;
    case 'event':
      if (
        keys(value, ['type', 'level', 'code', 'message', 'fields']) &&
        typeof value.level === 'string' &&
        ['debug', 'info', 'warn', 'error'].includes(value.level) &&
        typeof value.code === 'string' &&
        typeof value.message === 'string' &&
        (!('fields' in value) || (record(value.fields) && Object.values(value.fields).every((field) => field === null || ['string', 'number', 'boolean'].includes(typeof field))))
      )
        return;
      break;
    case 'deactivate':
      if (keys(value, ['type'])) return;
      break;
    case 'startupFailed':
      if (keys(value, ['type', 'detail']) && record(value.detail) && typeof value.detail.message === 'string') return;
      break;
  }
  throw error();
}

function configuration(options: NativeFrameSessionOptions): { url: string; limits: Limits } {
  try {
    const url = new URL(options.url);
    if (!['ws:', 'wss:'].includes(url.protocol) || url.username || url.password || url.href.includes('?') || url.href.includes('#') || !/^\/accounts\/[a-zA-Z0-9_-]{1,128}$/.test(url.pathname)) throw invalid();
    if (typeof options.token !== 'string' || !/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/.test(options.token)) throw invalid();
    if (!options.sink || typeof options.sink.onFrame !== 'function' || typeof options.sink.onNotice !== 'function' || typeof options.sink.onClose !== 'function' || (options.sink.onAccountAuth !== undefined && typeof options.sink.onAccountAuth !== 'function') || (options.socketFactory !== undefined && typeof options.socketFactory !== 'function')) throw invalid();
    if (options.signal !== undefined && (typeof options.signal.aborted !== 'boolean' || typeof options.signal.addEventListener !== 'function' || typeof options.signal.removeEventListener !== 'function')) throw invalid();
    const limits: Limits = {
      connectTimeoutMs: options.connectTimeoutMs ?? 10_000,
      maxQueuedMessages: options.maxQueuedMessages ?? 64,
      maxQueuedBytes: options.maxQueuedBytes ?? 32 * 1024 * 1024,
      maxFrameBytes: options.maxFrameBytes ?? 16 * 1024 * 1024,
      maxCommandBytes: options.maxCommandBytes ?? 64 * 1024,
      maxBufferedAmount: options.maxBufferedAmount ?? 1024 * 1024,
    };
    if (Object.values(limits).some((value) => !Number.isSafeInteger(value) || value <= 0) || limits.connectTimeoutMs > 0x7fffffff) throw invalid();
    return { url: url.href, limits };
  } catch {
    throw invalid();
  }
}

/**
 * One account connection, one fresh snapshot/baseline. Never reconnects or retries
 * commands. The embedding session owns authorization and downstream delivery.
 */
export class NativeFrameSession {
  private socket?: NativeFrameSocket;
  private timer?: ReturnType<typeof setTimeout>;
  private readonly connected: Promise<void>;
  private resolveConnected!: () => void;
  private rejectConnected!: (reason: Error) => void;
  private readonly queue: Delivery[] = [];
  private queuedBytes = 0;
  private queuedMessages = 0;
  private delivering = false;
  private opened = false;
  private receivedSnapshot = false;
  private isReady = false;
  private isClosed = false;

  private constructor(private readonly sink: NativeFrameSessionSink, private readonly limits: Limits, private readonly signal?: AbortSignal) {
    this.connected = new Promise<void>((resolve, reject) => {
      this.resolveConnected = resolve;
      this.rejectConnected = reject;
    });
  }

  static async connect(options: NativeFrameSessionOptions): Promise<NativeFrameSession> {
    const { url, limits } = configuration(options);
    const session = new NativeFrameSession(options.sink, limits, options.signal);
    session.start(url, options.token, options.socketFactory ?? ((address, protocols) => new WebSocket(address, protocols)));
    await session.connected;
    if (session.isClosed) throw error();
    return session;
  }

  get ready(): boolean { return this.isReady; }
  get closed(): boolean { return this.isClosed; }

  /** Preserve the exact JSON and caller IDs; the native typed decoder is authoritative. */
  sendCommand(rawJson: string): void {
    if (!this.isReady || this.isClosed) throw error();
    try {
      if (typeof rawJson !== 'string' || encoder.encode(rawJson).byteLength > this.limits.maxCommandBytes) throw error();
      const value: unknown = JSON.parse(rawJson);
      if (!record(value) || typeof value.op !== 'string' || !value.op.length) throw error();
    } catch {
      throw new Error('Invalid native frame command');
    }
    const socket = this.socket;
    const bytes = encoder.encode(rawJson).byteLength;
    if (!socket || socket.readyState !== 1 || !Number.isFinite(socket.bufferedAmount) || socket.bufferedAmount < 0 || bytes > this.limits.maxBufferedAmount - socket.bufferedAmount) {
      this.close();
      throw error();
    }
    try {
      socket.send(rawJson);
    } catch {
      this.close();
      throw error();
    }
  }

  close(): void {
    if (this.isClosed) return;
    this.isClosed = true;
    this.isReady = false;
    clearTimeout(this.timer);
    this.signal?.removeEventListener('abort', this.onSocketEnd);
    this.queue.length = 0;
    this.queuedBytes = this.queuedMessages = 0;
    this.rejectConnected(error());
    const socket = this.socket;
    if (socket) {
      socket.removeEventListener('open', this.onOpen);
      socket.removeEventListener('message', this.onMessage);
      socket.removeEventListener('error', this.onSocketEnd);
      socket.removeEventListener('close', this.onSocketEnd);
      try { socket.close(1000, 'session closed'); } catch { /* Socket errors must not expose connection credentials. */ }
    }
    // A caller callback must never turn teardown into an unhandled rejection.
    try { void Promise.resolve(this.sink.onClose(error())).catch(() => {}); } catch { /* Teardown is final. */ }
  }

  private start(url: string, token: string, factory: NativeFrameSocketFactory): void {
    this.signal?.addEventListener('abort', this.onSocketEnd, { once: true });
    if (this.signal?.aborted) { this.close(); return; }
    this.timer = setTimeout(() => this.close(), this.limits.connectTimeoutMs);
    try {
      const socket = factory(url, [PROTOCOL, ACCOUNT_AUTH_PROTOCOL, `bearer.${token}`]);
      // An injected factory can synchronously abort the owning downstream session.
      // Teardown may therefore have happened before a socket was available to close.
      if (this.isClosed) {
        try { socket.close(1000, 'session closed'); } catch { /* Teardown remains final. */ }
        return;
      }
      this.socket = socket;
      this.socket.binaryType = 'arraybuffer';
      this.socket.addEventListener('open', this.onOpen);
      this.socket.addEventListener('message', this.onMessage);
      this.socket.addEventListener('error', this.onSocketEnd);
      this.socket.addEventListener('close', this.onSocketEnd);
    } catch {
      this.close();
    }
  }

  private readonly onSocketEnd: EventListener = () => { this.close(); };

  private readonly onOpen: EventListener = () => {
    if (this.isClosed) return;
    if (this.opened || this.socket?.protocol !== PROTOCOL || this.socket.readyState !== 1) { this.close(); return; }
    this.opened = true;
  };

  private readonly onMessage: EventListener = (event) => {
    if (this.isClosed) return;
    try {
      if (!this.opened) throw error();
      const data: unknown = (event as MessageEvent<unknown>).data;
      let delivery: Delivery;
      if (typeof data === 'string') {
        const bytes = encoder.encode(data).byteLength;
        if (bytes > this.limits.maxFrameBytes || bytes > this.limits.maxQueuedBytes - this.queuedBytes) throw error();
        const auth = validateNotice(data);
        delivery = { text: data, bytes, auth };
      } else {
        let buffer: ArrayBuffer;
        if (data instanceof ArrayBuffer) buffer = data;
        else if (ArrayBuffer.isView(data)) {
          if (data.byteLength > this.limits.maxFrameBytes || data.byteLength > this.limits.maxQueuedBytes - this.queuedBytes) throw error();
          buffer = new Uint8Array(data.buffer, data.byteOffset, data.byteLength).slice().buffer;
        } else throw error(); // binaryType is arraybuffer; async Blob conversion can reorder delivery.
        const bytes = buffer.byteLength;
        if (bytes > this.limits.maxFrameBytes || bytes > this.limits.maxQueuedBytes - this.queuedBytes) throw error();
        const frame = new Frame(buffer);
        const snapshot = !this.receivedSnapshot;
        if (snapshot && !frame.bookDeltas().next().done) throw error();
        this.receivedSnapshot = true;
        delivery = { buffer, bytes, snapshot };
      }
      if (this.queuedMessages >= this.limits.maxQueuedMessages) throw error();
      this.queuedMessages++;
      this.queuedBytes += delivery.bytes;
      this.queue.push(delivery);
      void this.deliver();
    } catch {
      this.close();
    }
  };

  private async deliver(): Promise<void> {
    if (this.delivering || this.isClosed) return;
    this.delivering = true;
    try {
      while (!this.isClosed && this.queue.length) {
        const delivery = this.queue.shift()!;
        if ('buffer' in delivery) {
          await this.sink.onFrame(delivery.buffer, { snapshot: delivery.snapshot });
          if (this.isClosed) return;
          if (delivery.snapshot) {
            this.isReady = true;
            clearTimeout(this.timer);
            this.resolveConnected();
          }
        } else {
          if (delivery.auth) await this.sink.onAccountAuth?.(delivery.auth);
          if (this.isClosed) return;
          await this.sink.onNotice(delivery.text);
        }
        if (this.isClosed) return;
        this.queuedMessages--;
        this.queuedBytes -= delivery.bytes;
      }
    } catch {
      this.close();
    } finally {
      this.delivering = false;
    }
  }
}
