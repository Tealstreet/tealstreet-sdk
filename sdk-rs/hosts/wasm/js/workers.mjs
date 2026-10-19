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
import { EngineHost, bindSocketMessages } from './host.mjs';

/** Outbound sockets and requests from a Worker or Durable Object. */
export const workersTransport = () => ({
  now: () => Date.now(),
  connect(url, { onOpen, onMessage, onClose }) {
    let socket;
    let closed = false;
    let disposeMessages;
    // Workers open client WebSockets with an upgrade request over https.
    fetch(url.replace(/^ws/, 'http'), { headers: { Upgrade: 'websocket' } }).then(
      (response) => {
        socket = response.webSocket;
        if (!socket) {
          if (!closed) onClose(1006);
          return;
        }
        socket.accept();
        disposeMessages = bindSocketMessages(socket, { onMessage, onClose });
        if (closed) {
          disposeMessages();
          socket.close();
        } else onOpen();
      },
      () => { if (!closed) onClose(1006); },
    );
    return {
      send: (text) => socket?.send(text),
      close: () => {
        closed = true;
        disposeMessages?.();
        socket?.close();
      },
    };
  },
  async request({ method, url, headers, body }) {
    const response = await fetch(url, { method, headers, body });
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
 * A Durable Object class running one engine. `loadEngine` resolves the
 * wasm-bindgen module (`{ WasmEngine }`) the deployment bundles; `flush`
 * gives the store and book cadences in ms.
 */
export const createEngineObject = ({ loadEngine, flush = { store: 250, books: 60 } }) =>
  class EngineObject {
    constructor(state) {
      this.state = state;
      this.clients = new Set();
      this.host = undefined;
      this.timers = [];
    }

    async fetch(request) {
      if (request.headers.get('Upgrade') !== 'websocket') {
        return new Response('expected a websocket', { status: 426 });
      }
      const [client, server] = Object.values(new WebSocketPair());
      server.accept();
      this.clients.add(server);
      server.addEventListener('message', (event) => this.receive(server, event.data));
      server.addEventListener('close', () => this.leave(server));
      if (this.host) server.send(this.host.snapshot());
      await this.state.storage.setAlarm(Date.now() + KEEPALIVE_MS);
      return new Response(null, { status: 101, webSocket: client });
    }

    async receive(client, data) {
      if (typeof data !== 'string') return;
      const message = JSON.parse(data);
      if (message.op === 'start' && !this.host) {
        const { WasmEngine } = await loadEngine();
        this.host = new EngineHost(
          new WasmEngine(message.venue, JSON.stringify(message.options ?? {})),
          workersTransport(),
          {
            onFrame: (frame) => this.broadcast(frame),
            onLog: (level, text) => this.broadcast(JSON.stringify({ op: 'log', level, message: text })),
          },
        );
        this.host.start();
        this.timers.push(setInterval(() => this.host?.flush(), flush.store));
        if (flush.books < flush.store) this.timers.push(setInterval(() => this.host?.flush(true), flush.books));
        return;
      }
      if (message.op === 'command' && this.host) {
        this.host.command(message.command).then(
          (result) => client.send(JSON.stringify({ op: 'done', id: message.id, result })),
          (error) => client.send(JSON.stringify({ op: 'done', id: message.id, error: String(error?.message ?? error) })),
        );
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
    }

    async alarm() {
      if (this.clients.size > 0) await this.state.storage.setAlarm(Date.now() + KEEPALIVE_MS);
    }
  };
