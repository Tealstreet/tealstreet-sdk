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

/** Outbound sockets and requests from a Worker or Durable Object. */
export const workersTransport = () => ({
  webSocketHeaders: true,
  now: () => Date.now(),
  connect(url, { headers = {}, onOpen, onMessage, onClose }) {
    let socket;
    let receive;
    let localClose = false;
    let reported = false;
    const reportClose = (code, receivedAt) => {
      if (!reported) { reported = true; onClose(code, receivedAt); }
    };
    const close = (code, receivedAt) => {
      if (localClose || !receive) reportClose(code, receivedAt);
      else receive.close(code, receivedAt);
    };
    // Workers open client WebSockets with an upgrade request over https.
    fetch(url.replace(/^ws/, 'http'), { headers: { ...headers, Upgrade: 'websocket' } }).then(
      (response) => {
        socket = response.webSocket;
        if (!socket) return close(1006, Date.now());
        socket.accept();
        receive = socketMessages(onMessage, (_error, receivedAt) => {
          receive.close(1007, receivedAt);
          socket.close();
        }, reportClose);
        socket.addEventListener('message', (event) => receive(event.data, Date.now()));
        socket.addEventListener('close', (event) => close(event.code, Date.now()));
        socket.addEventListener('error', () => close(1006, Date.now()));
        if (localClose) { receive.cancel(); socket.close(); }
        else onOpen();
      },
      () => close(1006, Date.now()),
    );
    return {
      send: (text) => socket?.send(text),
      close: () => {
        localClose = true;
        receive?.cancel();
        socket?.close();
      },
    };
  },
  async request({ method, url, headers, body }) {
    const response = await fetch(url, { method, headers, body, signal: AbortSignal.timeout(20_000) });
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
  for (;;) {
    const { done, value } = await reader.read();
    if (done) return;
    yield value;
  }
}

/** While clients are connected the object wakes itself this often. */
const KEEPALIVE_MS = 30_000;

/**
 * A Durable Object class running one engine. `loadEngine(venue)` resolves the
 * venue-specific wasm-bindgen module (`{ WasmEngine }`) the deployment bundles; `flush`
 * gives the store and book cadences in ms.
 */
export const createEngineObject = ({ loadEngine, flush = { store: 250, books: 60 } }) =>
  class EngineObject {
    constructor(state) {
      this.state = state;
      this.clients = new Set();
      this.host = undefined;
      this.venue = undefined;
      this.starting = undefined;
      this.timers = [];
    }

    async fetch(request) {
      if (request.headers.get('Upgrade') !== 'websocket') {
        return new Response('expected a websocket', { status: 426 });
      }
      const [client, server] = Object.values(new WebSocketPair());
      server.accept();
      this.clients.add(server);
      server.addEventListener('message', (event) => {
        this.receive(server, event.data).catch((error) => {
          if (this.clients.has(server)) {
            server.send(JSON.stringify({ op: 'log', level: 'error', message: String(error?.message ?? error) }));
          }
        });
      });
      server.addEventListener('close', () => this.leave(server));
      if (this.host) server.send(this.host.snapshot());
      await this.state.storage.setAlarm(Date.now() + KEEPALIVE_MS);
      return new Response(null, { status: 101, webSocket: client });
    }

    async receive(client, data) {
      if (typeof data !== 'string') return;
      const message = JSON.parse(data);
      if (message.op === 'start') {
        if (this.host || this.starting) {
          if (message.venue !== (this.venue ?? this.starting.venue)) {
            throw new Error('this account already selected a different venue');
          }
          return this.starting?.promise;
        }
        const starting = { venue: message.venue };
        this.starting = starting;
        starting.promise = (async () => {
          const { WasmEngine } = await loadEngine(message.venue);
          // Leaving invalidates this attempt even if another client reconnects
          // before its module finishes loading.
          if (this.starting !== starting || this.clients.size === 0) return;
          const transport = workersTransport();
          const host = new EngineHost(
            new WasmEngine(message.venue, JSON.stringify({ ...message.options, privateWsHeaders: transport.webSocketHeaders })),
            transport,
            {
              onFrame: (frame) => this.broadcast(frame),
              onLog: (level, text) => this.broadcast(JSON.stringify({ op: 'log', level, message: text })),
            },
          );
          this.host = host;
          this.venue = message.venue;
          try {
            host.start();
          } catch (error) {
            host.dispose();
            this.host = undefined;
            this.venue = undefined;
            throw error;
          }
          this.timers.push(setInterval(() => this.host?.flush(), flush.store));
          if (flush.books < flush.store) this.timers.push(setInterval(() => this.host?.flush(true), flush.books));
        })();
        try {
          await starting.promise;
        } finally {
          if (this.starting === starting) this.starting = undefined;
        }
        return;
      }
      if (message.op === 'command') {
        try {
          if (!this.host && this.starting) await this.starting.promise;
          if (!this.clients.has(client)) return;
          if (!this.host) throw new Error('the engine has not started');
          const result = await this.host.command(message.command);
          if (this.clients.has(client)) client.send(JSON.stringify({ op: 'done', id: message.id, result }));
        } catch (error) {
          if (this.clients.has(client)) {
            client.send(JSON.stringify({ op: 'done', id: message.id, error: String(error?.message ?? error) }));
          }
        }
      }
    }

    broadcast(message) {
      for (const client of this.clients) client.send(message);
    }

    /** The last client leaving stops the engine and drops its credentials. */
    leave(client) {
      this.clients.delete(client);
      if (this.clients.size > 0) return;
      this.timers.forEach(clearInterval);
      this.timers = [];
      this.host?.dispose();
      this.host = undefined;
      this.venue = undefined;
      this.starting = undefined;
    }

    async alarm() {
      if (this.clients.size > 0) await this.state.storage.setAlarm(Date.now() + KEEPALIVE_MS);
    }
  };
