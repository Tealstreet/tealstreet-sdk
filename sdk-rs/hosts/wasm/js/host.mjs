// Runs a WasmEngine against a transport: executes its actions (sockets, HTTP,
// timers) and feeds the results back in. The same host serves browser workers,
// Cloudflare Workers, and the replay harness; only the transport differs.
//
// transport = {
//   now(): number,
//   connect(url, { onOpen, onMessage, onClose }): { send(text), close() },
//   request({ method, url, headers, body }):
//     Promise<{ status, headers?: string, body: Uint8Array | AsyncIterable<Uint8Array> }>,
//     (`headers` as `name: value` lines; a streamed body is fed to the engine
//     chunk by chunk so large lists never sit whole in its memory),
//   setTimeout(fn, ms), clearTimeout(handle),
// }

const CHUNK_BYTES = 16 * 1024;

export class EngineHost {
  constructor(engine, transport, handlers = {}) {
    this.engine = engine;
    this.transport = transport;
    this.handlers = handlers;
    this.sockets = new Map();
    this.timers = new Map();
    this.commands = new Map();
    this.nextCommand = 1;
    this.disposed = false;
  }

  start() {
    if (this.disposed) return;
    this.run(this.engine.start(this.transport.now()));
  }

  /** Send a command; resolves with its result for commands that report one. */
  command(command) {
    if (this.disposed) return Promise.reject(new Error('engine disposed'));
    const reply = command.op === 'setTickerSymbols' || command.op === 'subscribeOrderBook' || command.op === 'unsubscribeOrderBook'
      || command.op === 'subscribeCandles' || command.op === 'unsubscribeCandles'
      ? undefined
      : this.nextCommand++;
    const promise = reply === undefined
      ? Promise.resolve()
      : new Promise((resolve, reject) => this.commands.set(reply, { resolve, reject }));
    try {
      this.run(this.engine.command(JSON.stringify(reply === undefined ? command : { ...command, command: reply }), this.transport.now()));
    } catch (error) {
      if (reply === undefined) return Promise.reject(error);
      const pending = this.commands.get(reply);
      this.commands.delete(reply);
      pending?.reject(error);
    }
    return promise;
  }

  /**
   * Hand everything that changed since the previous flush to `onFrame` as one
   * ArrayBuffer (read it with wire.mjs's Frame; transfer it, don't copy it).
   * Call once per flush interval; with `marketOnly`, only books, candles and
   * fills go out, for flushing those at the venue's faster book cadence.
   */
  flush(marketOnly = false) {
    const frame = this.engine.takeFrame(this.transport.now(), marketOnly);
    if (frame) this.handlers.onFrame?.(frame.buffer);
  }

  /** The whole current state as one frame (an ArrayBuffer), for a consumer
   * that starts late or skipped frames; later flushes apply on top of it. */
  snapshot() {
    return this.engine.snapshotFrame(this.transport.now()).buffer;
  }

  dispose() {
    this.disposed = true;
    for (const pending of this.commands.values()) pending.reject(new Error('engine disposed'));
    this.commands.clear();
    for (const socket of this.sockets.values()) socket.close();
    for (const handle of this.timers.values()) this.transport.clearTimeout(handle);
    this.sockets.clear();
    this.timers.clear();
  }

  /** Feed a response to the engine in chunks of at most `CHUNK_BYTES`. */
  async receive(id, { status, headers = '', body }) {
    if (this.disposed) return;
    const { engine, transport } = this;
    this.run(engine.httpStart(id, status, headers, transport.now()));
    try {
      if (body instanceof Uint8Array) {
        for (let at = 0; at < body.length; at += CHUNK_BYTES) {
          if (this.disposed) return;
          this.run(engine.httpChunk(id, body.subarray(at, at + CHUNK_BYTES), transport.now()));
        }
      } else {
        for await (const chunk of body) {
          for (let at = 0; at < chunk.length; at += CHUNK_BYTES) {
            if (this.disposed) return;
            this.run(engine.httpChunk(id, chunk.subarray(at, at + CHUNK_BYTES), transport.now()));
          }
        }
      }
    } catch (error) {
      if (this.disposed) return;
      this.run(engine.httpFailed(id, String(error?.message ?? error), transport.now()));
      return;
    }
    if (!this.disposed) this.run(engine.httpEnd(id, transport.now()));
  }

  run(actionsJson) {
    if (this.disposed || !actionsJson) return;
    for (const action of JSON.parse(actionsJson)) this.execute(action);
  }

  execute(action) {
    if (this.disposed) return;
    const { engine, transport } = this;
    switch (action.op) {
      case 'wsConnect': {
        const id = action.socket;
        if (action.headers?.length && !transport.webSocketHeaders) {
          this.handlers.onLog?.('warning', 'WebSocket upgrade headers unavailable; continuing REST reconciliation');
          transport.setTimeout(() => { if (!this.disposed) this.run(engine.wsClose(id, 1006, transport.now())); }, 0);
          break;
        }
        const socket = transport.connect(action.url, {
          headers: action.headers && Object.fromEntries(action.headers),
          onOpen: () => { if (!this.disposed) this.run(engine.wsOpen(id, transport.now())); },
          onMessage: (text) => { if (!this.disposed) this.run(engine.wsMessage(id, text, transport.now())); },
          onClose: (code) => {
            if (this.disposed) return;
            this.sockets.delete(id);
            this.run(engine.wsClose(id, code ?? 1006, transport.now()));
          },
        });
        this.sockets.set(id, socket);
        break;
      }
      case 'wsSend':
        this.sockets.get(action.socket)?.send(action.text);
        break;
      case 'wsClose':
        this.sockets.get(action.socket)?.close();
        this.sockets.delete(action.socket);
        break;
      case 'http': {
        const id = action.request;
        transport
          .request({
            method: action.method,
            url: action.url,
            headers: Object.fromEntries(action.headers),
            body: action.body,
          })
          .then(
            (response) => this.receive(id, response),
            (error) => { if (!this.disposed) this.run(engine.httpFailed(id, String(error?.message ?? error), transport.now())); },
          );
        break;
      }
      case 'setTimer': {
        const id = action.timer;
        const handle = transport.setTimeout(() => {
          this.timers.delete(id);
          if (this.disposed) return;
          this.run(engine.timer(id, transport.now()));
        }, Math.max(0, action.at - transport.now()));
        this.timers.set(id, handle);
        break;
      }
      case 'clearTimer':
        transport.clearTimeout(this.timers.get(action.timer));
        this.timers.delete(action.timer);
        break;
      case 'log':
        this.handlers.onLog?.(action.level, action.message);
        break;
      case 'done': {
        const pending = this.commands.get(action.command);
        this.commands.delete(action.command);
        if (!pending) break;
        if (action.error !== undefined) {
          const error = new Error(action.error);
          try {
            const details = JSON.parse(action.error);
            if (typeof details.message === 'string' && Array.isArray(details.partialOrderIds) &&
                details.partialOrderIds.every((id) => typeof id === 'string')) {
              error.message = details.message;
              error.partialOrderIds = details.partialOrderIds;
              error.partialOrderCount = details.partialOrderCount;
              error.totalOrderCount = details.totalOrderCount;
              error.failedOrderIndex = details.failedOrderIndex;
            }
          } catch { /* Plain venue errors carry only their message. */ }
          pending.reject(error);
        }
        else pending.resolve(action.result === undefined ? undefined : JSON.parse(action.result));
        break;
      }
      default:
        throw new Error(`unknown engine action: ${action.op}`);
    }
  }
}

/** A fetch body's chunks; an empty body yields nothing. */
async function* chunks(stream) {
  if (!stream) return;
  const reader = stream.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) return;
    yield value;
  }
}

/** Deliver compressed binary messages and closure in receive order on every JS host. */
export const bindSocketMessages = (socket, { onMessage, onClose, onError }) => {
  let pending;
  let disposed = false;
  let ending = false;
  let failed = false;
  socket.addEventListener('message', (event) => {
    if (ending || disposed || failed) return;
    if (typeof event.data === 'string' && !pending) {
      onMessage(event.data);
      return;
    }
    // Chain binary decoding so a later snapshot cannot overtake an earlier one.
    const task = (pending ?? Promise.resolve()).then(async () => {
      if (disposed || failed) return;
      if (typeof event.data === 'string') return onMessage(event.data);
      const blob = event.data instanceof Blob ? event.data : new Blob([event.data]);
      const bytes = new Uint8Array(await blob.arrayBuffer());
      const stream = bytes[0] === 0x1f && bytes[1] === 0x8b
        ? blob.stream().pipeThrough(new DecompressionStream('gzip'))
        : bytes[0] === 0x78 && ((bytes[0] << 8) + bytes[1]) % 31 === 0
          ? blob.stream().pipeThrough(new DecompressionStream('deflate')) : blob.stream();
      const reader = stream.getReader();
      const decoder = new TextDecoder('utf-8', { fatal: true });
      let size = 0;
      let text = '';
      try {
        for (;;) {
          const { done, value } = await reader.read();
          if (done || disposed) break;
          size += value.byteLength;
          if (size > 16 * 1024 * 1024) throw new Error('websocket message exceeds 16 MiB');
          text += decoder.decode(value, { stream: true });
        }
        if (!disposed) text += decoder.decode();
      } finally {
        await reader.cancel();
      }
      if (!disposed) onMessage(text);
    }).catch((error) => {
      failed = true;
      if (!disposed) {
        try { onError?.(error); } finally { socket.close(); }
      }
    });
    pending = task;
    task.finally(() => { if (pending === task) pending = undefined; });
  });
  const end = (code) => {
    if (ending) return;
    ending = true;
    const deliver = () => { if (!disposed) onClose(code); };
    if (pending) pending = pending.then(deliver);
    else deliver();
  };
  socket.addEventListener('close', (event) => end(event.code));
  socket.addEventListener('error', () => end(1006));
  return () => { disposed = true; };
};

/** Ordered decoding for transports that pass raw message data as a callback. */
export const socketMessages = (onMessage, onError) => {
  let receive;
  bindSocketMessages({
    addEventListener(type, listener) { if (type === 'message') receive = listener; },
    close() {},
  }, { onMessage, onError, onClose() {} });
  return (data) => receive({ data });
};

/** Transport over the runtime's own WebSocket, fetch and timers. */
export const nativeTransport = () => {
  let Socket = globalThis.WebSocket;
  let webSocketHeaders = false;
  try {
    const module = globalThis.process?.getBuiltinModule?.('module');
    if (module?.createRequire) {
      Socket = module.createRequire(import.meta.url)('ws');
      webSocketHeaders = true;
    }
  } catch { /* Browser and restricted renderer transports keep REST fallback. */ }
  return {
  webSocketHeaders,
  now: () => Date.now(),
  connect(url, { headers = {}, onOpen, onMessage, onClose }) {
    if (Object.keys(headers).length && !webSocketHeaders) throw new Error('WebSocket upgrade headers unavailable');
    const ws = webSocketHeaders ? new Socket(url, { headers: { 'User-Agent': 'Tealstreet', ...headers } }) : new Socket(url);
    let closed = false;
    let reported = false;
    const reportClose = (code) => { if (!reported) { reported = true; closed = true; onClose(code); } };
    ws.addEventListener('open', () => { if (!closed) onOpen(); });
    const dispose = bindSocketMessages(ws, {
      onMessage: (text) => { if (!closed) onMessage(text); },
      onClose: reportClose,
    });
    // Local close disposes queued decoding, but still acknowledges closure once.
    ws.addEventListener('error', () => { if (closed) reportClose(1006); });
    ws.addEventListener('close', (event) => { if (closed) reportClose(event.code); });
    return { send: (text) => ws.send(text), close: () => { closed = true; dispose(); ws.close(); } };
  },
  async request({ method, url, headers, body }) {
    const response = await fetch(url, { method, headers, body, signal: AbortSignal.timeout(20_000) });
    const lines = [];
    response.headers.forEach((value, name) => lines.push(`${name}: ${value}`));
    return { status: response.status, headers: lines.join('\n'), body: chunks(response.body) };
  },
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (handle) => clearTimeout(handle),
};
};
