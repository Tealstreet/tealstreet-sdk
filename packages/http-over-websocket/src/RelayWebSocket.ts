// A WebSocket-shaped client for the tradetunnel's `rpc.relay` mode.
//
// The tunnel pipes a browser socket to a venue's PRIVATE WebSocket through the
// private proxy fleet, so the socket egresses from an IP the user already
// whitelists instead of from their own ISP (see
// docs/plans/TRADETUNNEL_WS_RELAY_v1.md). The wire protocol is two frames after
// connect — `rpc.auth` with the proxy JWT, then `rpc.relay` naming the
// upstream — after which the connection is a verbatim pipe and the tunnel's
// own frames stop.
//
// This class hides that preface behind the standard WebSocket surface: it
// reports `open` only once the tunnel has answered `rpc.relay` with
// `status: "open"`, buffers anything sent before then, and forwards every later
// frame untouched. Every failure before `open` (bad token, relay refused, relay
// disabled on that tunnel, timeout) surfaces as `error` + `close` with NO
// `message` ever dispatched, which is exactly what `WsDirectFallback` counts
// as a silent connect — so the caller's fallback chain advances past the relay
// on its own.
//
// It is constructed by the vendored ReconnectingWebSocket through its
// `WebSocket` option, from a URL of the form
//   wss://fsjnpy143pbz.<region>.tealstreet.io/tunnel?ts-relay=<encoded upstream>
// (`buildRelayUrl`), so the exchange classes keep treating it as just another
// host in `transport/tealstreet/routing/routingTable.ts`. In Node/Bun the inner socket is the `ws`
// client with whatever upgrade options the caller passed (Origin/UA headers,
// ALPN http/1.1 — the tunnel's WAF needs them); in browsers it is the native
// WebSocket, which ignores those options.

import { bindProxyToken } from './config';
import { parseRelayUrl } from './tunnelEndpoints';

export { buildRelayUrl, isRelayUrl, parseRelayUrl, RELAY_QUERY_PARAM } from './tunnelEndpoints';

const HANDSHAKE_TIMEOUT_MS = 15_000;
const MAX_QUEUED_SENDS = 100;

// Import ws for Node.js/Bun: Bun's native WebSocket lacks ALPNProtocols, which
// the tunnel needs to force HTTP/1.1. Same pattern as HOWWebSocket.
let NodeWebSocket: (new (url: string, protocols?: string | string[], options?: unknown) => WebSocket) | undefined;
const isNodeOrBun = typeof process !== 'undefined' && (process.versions?.node || (process as any).isBun);
if (isNodeOrBun) {
  try {
    // Dynamic require so browser bundles never see `ws`.
    NodeWebSocket = require('ws');
  } catch {
    // ws not available: fall back to the global WebSocket below.
  }
}

type SendData = string | ArrayBufferLike | Blob | ArrayBufferView;

const makeCloseEvent = (code: number, reason: string, wasClean: boolean): Event => {
  const CloseEventCtor = (globalThis as any).CloseEvent as (new (type: string, init: any) => Event) | undefined;
  if (CloseEventCtor) {
    try {
      return new CloseEventCtor('close', { code, reason, wasClean });
    } catch {
      // fall through to the plain event below
    }
  }
  const event = new Event('close');
  Object.assign(event, { code, reason, wasClean });
  return event;
};

const makeErrorEvent = (message: string): Event => {
  const event = new Event('error');
  Object.assign(event, { message, error: new Error(message) });
  return event;
};

export class RelayWebSocket extends EventTarget {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  readonly CONNECTING = 0;
  readonly OPEN = 1;
  readonly CLOSING = 2;
  readonly CLOSED = 3;

  readonly url: string;
  readonly tunnelUrl: string;
  readonly upstream: string;
  readyState = RelayWebSocket.CONNECTING;
  binaryType: BinaryType = 'blob';
  readonly extensions = '';
  readonly protocol = '';

  onopen: ((event: Event) => void) | null = null;
  onclose: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  private readonly inner: WebSocket;
  private readonly queue: SendData[] = [];
  private authSent = false;
  private closeDispatched = false;
  private handshakeTimer: ReturnType<typeof setTimeout> | undefined;
  // Read on the inner socket's open event, outside the creating context.
  private readonly getProxyToken = bindProxyToken();

  constructor(url: string, protocols?: string | string[], nodeOptions?: unknown, WebSocketCtor?: typeof WebSocket) {
    super();
    const { tunnelUrl, upstream } = parseRelayUrl(url);
    this.url = url;
    this.tunnelUrl = tunnelUrl;
    this.upstream = upstream;

    const Socket = WebSocketCtor ?? NodeWebSocket;
    this.inner = Socket
      ? new (Socket as new (url: string, protocols?: string | string[], options?: unknown) => WebSocket)(
          tunnelUrl,
          protocols ?? [],
          nodeOptions,
        )
      : new WebSocket(tunnelUrl);
    this.inner.addEventListener('open', this.onInnerOpen);
    this.inner.addEventListener('message', this.onInnerMessage);
    this.inner.addEventListener('close', this.onInnerClose as EventListener);
    this.inner.addEventListener('error', this.onInnerError);
    this.handshakeTimer = setTimeout(() => this.fail('relay handshake timed out'), HANDSHAKE_TIMEOUT_MS);
  }

  get bufferedAmount(): number {
    return this.inner.bufferedAmount ?? 0;
  }

  send(data: SendData): void {
    if (this.readyState === RelayWebSocket.OPEN) {
      this.inner.send(data as any);
      return;
    }
    if (this.readyState === RelayWebSocket.CONNECTING) {
      if (this.queue.length >= MAX_QUEUED_SENDS) this.queue.shift();
      this.queue.push(data);
      return;
    }
    throw new Error('RelayWebSocket is not open');
  }

  close(code?: number, reason?: string): void {
    if (this.readyState === RelayWebSocket.CLOSING || this.readyState === RelayWebSocket.CLOSED) return;
    this.readyState = RelayWebSocket.CLOSING;
    this.clearHandshakeTimer();
    try {
      if (code === undefined) this.inner.close();
      else this.inner.close(code, reason);
    } catch {
      this.finish(1006, 'close failed', false);
    }
  }

  // ---- inner socket ----

  private onInnerOpen = () => {
    if (this.readyState !== RelayWebSocket.CONNECTING) return;
    this.getProxyToken()
      .then((token) => {
        if (this.readyState !== RelayWebSocket.CONNECTING) return;
        this.authSent = true;
        this.inner.send(JSON.stringify({ type: 'rpc.auth', token, source: 'worker' }));
      })
      .catch(() => this.fail('proxy token unavailable'));
  };

  private onInnerMessage = (event: Event) => {
    const data = (event as MessageEvent).data;
    if (this.readyState === RelayWebSocket.OPEN) {
      this.dispatchMessage(data);
      return;
    }
    if (this.readyState !== RelayWebSocket.CONNECTING) return;

    // Handshake phase: only the tunnel's own JSON frames arrive here.
    let json: any;
    try {
      json = JSON.parse(typeof data === 'string' ? data : String(data));
    } catch {
      return;
    }
    switch (json?.type) {
      case 'rpc.auth':
        if (!this.authSent) return; // an IP-whitelisted tunnel greets unprompted; wait for our own
        if (json.status === 'success') {
          this.inner.send(JSON.stringify({ type: 'rpc.relay', upstream: this.upstream }));
        } else {
          this.fail(`tunnel auth failed: ${json.error ?? 'unknown'}`);
        }
        return;
      case 'rpc.relay':
        if (json.status === 'open') this.becomeOpen();
        else this.fail(`relay refused: ${json.error ?? 'unknown'}`);
        return;
      default:
        // serverTime / heartbeat / rpc.ping: tunnel chatter, never surfaced.
        return;
    }
  };

  private onInnerClose = (event: Event) => {
    const {
      code = 1006,
      reason = '',
      wasClean = false,
    } = event as unknown as {
      code?: number;
      reason?: string;
      wasClean?: boolean;
    };
    this.finish(code, reason, wasClean);
  };

  private onInnerError = () => {
    if (this.readyState === RelayWebSocket.CLOSED) return;
    this.dispatchError('relay transport error');
  };

  // ---- state transitions ----

  private becomeOpen() {
    this.clearHandshakeTimer();
    this.readyState = RelayWebSocket.OPEN;
    const event = new Event('open');
    this.onopen?.(event);
    this.dispatchEvent(event);
    for (const data of this.queue.splice(0)) this.inner.send(data as any);
  }

  private fail(message: string) {
    if (this.readyState === RelayWebSocket.CLOSED) return;
    this.dispatchError(message);
    this.readyState = RelayWebSocket.CLOSING;
    this.clearHandshakeTimer();
    try {
      this.inner.close();
    } catch {
      // ignore
    }
    // The inner close event (if it ever comes) is idempotent via closeDispatched;
    // make sure a socket the tunnel never closes still reports closed.
    this.finish(4000, message, false);
  }

  private finish(code: number, reason: string, wasClean: boolean) {
    if (this.closeDispatched) return;
    this.closeDispatched = true;
    this.clearHandshakeTimer();
    this.readyState = RelayWebSocket.CLOSED;
    this.inner.removeEventListener('open', this.onInnerOpen);
    this.inner.removeEventListener('message', this.onInnerMessage);
    this.inner.removeEventListener('close', this.onInnerClose as EventListener);
    // The error listener stays: closing a still-connecting Node `ws` emits its abort
    // error on a later tick, and an emitter with no listener throws it uncaught.
    const event = makeCloseEvent(code, reason, wasClean);
    this.onclose?.(event);
    this.dispatchEvent(event);
  }

  private dispatchMessage(data: unknown) {
    const event = new MessageEvent('message', { data });
    this.onmessage?.(event);
    this.dispatchEvent(event);
  }

  private dispatchError(message: string) {
    const event = makeErrorEvent(message);
    this.onerror?.(event);
    this.dispatchEvent(event);
  }

  private clearHandshakeTimer() {
    if (this.handshakeTimer) clearTimeout(this.handshakeTimer);
    this.handshakeTimer = undefined;
  }
}
