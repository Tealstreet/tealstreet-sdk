// Cloudflare Workers host: the same wasm engine and EngineHost as the browser,
// with a transport over Workers' outbound WebSocket and streaming fetch, and a
// Durable Object that runs one account's engine for its connected clients.
//
// Clients open a WebSocket to the object and speak JSON:
//   -> { op: 'start', venue, options }      options as WasmEngine takes them
//   -> { op: 'command', id, command }       any EngineHost command
//   <- binary messages: flush frames (a snapshot first), read with wire.mjs
//   <- { op: 'done', id, result } | { op: 'done', id, error } | { op: 'log', level, message }
// Credentials arrive with 'start' and live only in the object's memory.
import { EngineHost, socketMessages } from './host.mjs';

/** Header-capable WebSocket constructor for HOW/relay in Workers. */
export class WorkersWebSocket extends EventTarget {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  CONNECTING = 0;
  OPEN = 1;
  CLOSING = 2;
  CLOSED = 3;
  readyState = 0;
  onopen = null;
  onmessage = null;
  onclose = null;
  onerror = null;
  socket = undefined;
  binary = 'arraybuffer';
  abort = new AbortController();

  constructor(url, protocols = [], options = {}) {
    super();
    this.url = url;
    const headers = { ...options.headers, Upgrade: 'websocket' };
    const selectedProtocols = typeof protocols === 'string' ? protocols : protocols.join(', ');
    if (selectedProtocols) headers['Sec-WebSocket-Protocol'] = selectedProtocols;
    fetch(url.replace(/^ws/, 'http'), { headers, signal: this.abort.signal, redirect: 'error' }).then(
      (response) => {
        const socket = response.webSocket;
        if (!socket) return this.fail();
        this.socket = socket;
        if (this.readyState !== this.CONNECTING) {
          try { socket.close(); } catch { /* already closed */ }
          return;
        }
        socket.binaryType = this.binary;
        socket.addEventListener('message', (event) => {
          if (this.readyState === this.OPEN) this.emit(new MessageEvent('message', { data: event.data }));
        });
        socket.addEventListener('close', (event) => this.finish(event.code ?? 1006, event.reason ?? '', event.wasClean ?? false));
        socket.addEventListener('error', () => this.fail());
        socket.accept();
        this.readyState = this.OPEN;
        this.emit(new Event('open'));
      },
      () => { if (this.readyState !== this.CLOSED) this.fail(); },
    ).catch(() => { if (this.readyState !== this.CLOSED) this.fail(); });
  }

  get binaryType() { return this.binary; }
  set binaryType(value) {
    this.binary = value;
    if (this.socket) this.socket.binaryType = value;
  }
  get bufferedAmount() { return this.socket?.bufferedAmount ?? 0; }
  get extensions() { return this.socket?.extensions ?? ''; }
  get protocol() { return this.socket?.protocol ?? ''; }

  emit(event) {
    this[`on${event.type}`]?.(event);
    this.dispatchEvent(event);
  }

  finish(code, reason, wasClean) {
    if (this.readyState === this.CLOSED) return;
    this.readyState = this.CLOSED;
    const event = new Event('close');
    Object.assign(event, { code, reason, wasClean });
    this.emit(event);
  }

  fail() {
    if (this.readyState === this.CLOSED) return;
    this.emit(new Event('error'));
    this.finish(1006, 'WebSocket transport failed', false);
    try { this.socket?.close(); } catch { /* transport already failed */ }
  }

  send(data) {
    if (this.readyState !== this.OPEN) throw new Error('WebSocket is not open');
    this.socket.send(data);
  }

  close(code = 1000, reason = '') {
    if (this.readyState === this.CLOSED || this.readyState === this.CLOSING) return;
    const connecting = this.readyState === this.CONNECTING;
    this.readyState = this.CLOSING;
    this.abort.abort();
    try { this.socket?.close(code, reason); } catch { /* make close observable below */ }
    if (connecting || !this.socket) this.finish(code, reason, true);
  }

  terminate() {
    this.close();
    this.finish(1000, '', true);
  }
}

/** Outbound sockets and requests from a Worker or Durable Object. */
export const workersTransport = () => ({
  webSocketHeaders: true,
  serverHeaders: true,
  WebSocket: WorkersWebSocket,
  now: () => Date.now(),
  connect(url, { headers = {}, binary = false, onOpen, onMessage, onBinary, onClose }) {
    const socket = new WorkersWebSocket(url, [], { headers });
    let closed = false;
    const finish = (code) => {
      if (closed) return;
      closed = true;
      onClose(code);
    };
    const receive = socketMessages(
      (data) => { if (!closed) { if (typeof data === 'string') onMessage(data); else onBinary?.(data); } },
      () => { if (!closed) { finish(1007); socket.close(); } },
      binary,
    );
    socket.onopen = () => { if (!closed) onOpen(); };
    socket.onmessage = (event) => { if (!closed) receive(event.data); };
    socket.onclose = (event) => finish(event.code);
    return {
      send: (text) => { if (!closed) socket.send(text); },
      close: () => {
        closed = true;
        socket.close();
      },
    };
  },
  async request({ method, url, headers, body, signal, route, timeoutMs }) {
    if (route === 'publicCors') throw new Error('Public CORS request routing is not configured');
    const deadline = AbortSignal.timeout(Math.max(1, timeoutMs ?? 20_000));
    const response = await fetch(url, { method, headers, body, signal: signal ? AbortSignal.any([signal, deadline]) : deadline, redirect: 'error' });
    const lines = [];
    response.headers.forEach((value, name) => lines.push(`${name}: ${value}`));
    return { status: response.status, headers: lines.join('\n'), body: chunks(response.body) };
  },
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (handle) => clearTimeout(handle),
});

async function* chunks(stream) {
  if (!stream) return;
  const reader = stream.getReader();
  let complete = false;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) { complete = true; return; }
      yield value;
    }
  } finally {
    if (!complete) await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

/** While clients are connected the object wakes itself this often. */
const KEEPALIVE_MS = 30_000;

/**
 * A Durable Object class running one engine. `loadEngine` resolves the
 * wasm-bindgen module (`{ WasmEngine }`) the deployment bundles; `flush`
 * gives the store and book cadences in ms. Account-backed deployments supply
 * `createTransport(venue, options, raw)` from safe-cex's createEngineTransport;
 * the default is an explicitly direct engine host. The returned transport
 * supplies engineOptions for protocol choices tied to its resolved routes,
 * overriding conflicting caller flags before engine construction.
 */
export const createEngineObject = ({ loadEngine, createTransport, accountToken, flush = { store: 250, books: 60 } }) =>
  class EngineObject {
    constructor(state, env) {
      this.state = state;
      this.env = env;
      this.clients = new Set();
      this.host = undefined;
      this.starting = undefined;
      this.timers = [];
    }

    async fetch(request) {
      if (request.headers.get('Upgrade')?.toLowerCase() !== 'websocket') {
        return new Response('expected a websocket', { status: 426 });
      }
      // Deployment owns the token for this object's account. A client cannot claim it.
      let token;
      try { token = await accountToken?.(this.state, this.env); } catch { /* fail closed */ }
      if (typeof token !== 'string' || !token || request.headers.get('Authorization') !== `Bearer ${token}`) {
        return new Response('unauthorized', { status: 401 });
      }
      const [client, server] = Object.values(new WebSocketPair());
      server.accept();
      this.clients.add(server);
      server.addEventListener('message', (event) => { void this.receive(server, event.data); });
      server.addEventListener('close', () => this.leave(server));
      if (this.host) server.send(this.host.snapshot());
      await this.state.storage.setAlarm(Date.now() + KEEPALIVE_MS);
      return new Response(null, { status: 101, webSocket: client });
    }

    reply(client, message) {
      if (this.clients.has(client)) {
        try { client.send(JSON.stringify(message)); } catch { this.leave(client); }
      }
    }

    async receive(client, data) {
      if (!this.clients.has(client) || typeof data !== 'string') return;
      let message;
      try { message = JSON.parse(data); } catch {
        this.reply(client, { op: 'log', level: 'error', message: 'Invalid engine message' });
        return;
      }
      if (!message || typeof message !== 'object' || Array.isArray(message)) return;
      if (message.op === 'start') {
        if (this.host || this.starting) {
          this.reply(client, { op: 'log', level: 'error', message: 'Engine is already started or starting' });
          return;
        }
        if (typeof message.venue !== 'string' || (message.options != null &&
          (typeof message.options !== 'object' || Array.isArray(message.options)))) {
          this.reply(client, { op: 'log', level: 'error', message: 'Invalid engine options' });
          return;
        }
        const attempt = {};
        this.starting = attempt;
        let host;
        try {
          const { WasmEngine } = await loadEngine();
          if (this.starting !== attempt || !this.clients.has(client)) return;
          if (!createTransport && (message.options?.tealstreetProxy || message.options?.tealstreetTunnel
            || message.options?.tealstreetWsRoute || message.options?.overrideProxyUrl || message.options?.bybitSiteId)) {
            this.reply(client, { op: 'log', level: 'error', message: 'Engine routing requires a configured createTransport factory' });
            return;
          }
          const raw = workersTransport();
          const transport = createTransport ? createTransport(message.venue, message.options, raw) : raw;
          if (this.starting !== attempt || !this.clients.has(client)) return;
          host = new EngineHost(
            new WasmEngine(message.venue, JSON.stringify({ ...message.options, ...transport.engineOptions, privateWsHeaders: transport.webSocketHeaders })),
            transport,
            {
              onFrame: (frame) => this.broadcast(frame),
              onDeactivate: () => this.broadcast(JSON.stringify({ op: 'deactivate' })),
              onLog: (level, text) => this.broadcast(JSON.stringify({ op: 'log', level, message: text })),
            },
          );
          if (this.starting !== attempt || !this.clients.has(client)) { host.dispose(); return; }
          this.host = host;
          host.start();
          if (this.host !== host) return;
          this.timers.push(setInterval(() => this.host?.flush(), flush.store));
          if (flush.books < flush.store) this.timers.push(setInterval(() => this.host?.flush(true), flush.books));
        } catch {
          host?.dispose();
          if (this.host === host) this.host = undefined;
          this.reply(client, { op: 'log', level: 'error', message: 'Engine start failed' });
        } finally {
          if (this.starting === attempt) this.starting = undefined;
        }
        return;
      }
      if (message.op === 'command' && this.host) {
        if (message.command?.op === 'publicFeedUpdate') {
          this.reply(client, { op: 'done', id: message.id, error: 'public feed updates require a trusted host provider' });
          return;
        }
        const host = this.host;
        try {
          const result = await host.command(message.command);
          if (this.host === host) this.reply(client, { op: 'done', id: message.id, result });
        } catch (error) {
          if (this.host === host) this.reply(client, { op: 'done', id: message.id, error: String(error?.message ?? error) });
        }
      }
    }

    broadcast(message) {
      for (const client of this.clients) {
        try { client.send(message); } catch { this.leave(client); }
      }
    }

    /** The last client leaving stops the engine and drops its credentials. */
    leave(client) {
      this.clients.delete(client);
      if (this.clients.size > 0) return;
      this.starting = undefined;
      this.timers.forEach(clearInterval);
      this.timers = [];
      this.host?.dispose();
      this.host = undefined;
    }

    async alarm() {
      if (this.clients.size > 0) await this.state.storage.setAlarm(Date.now() + KEEPALIVE_MS);
    }
  };
