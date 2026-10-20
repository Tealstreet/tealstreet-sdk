// Runs a WasmEngine against a transport: executes its actions (sockets, HTTP,
// timers) and feeds the results back in. The same host serves browser workers,
// Cloudflare Workers, and the replay harness; only the transport differs.
//
// transport = {
//   now(): number,
//   connect(url, { onOpen, onMessage, onClose }): { send(text), close() },
//   request({ method, url, headers, body, signal }):
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
    this.requests = new Map();
    this.nextCommand = 1;
    this.disposed = false;
    this.wsEncoder = undefined;
    this.wsScratch = undefined;
    this.wsScratchBusy = false;
  }

  start() {
    if (this.disposed) return;
    this.run(this.engine.start(this.transport.now()));
  }

  /** Send a command; resolves with its result for commands that report one. */
  command(command) {
    if (this.disposed) return Promise.reject(new Error('engine disposed'));
    const reply = command.op === 'setTickerSymbols' || command.op === 'setTickInterval' || command.op === 'subscribeOrderBook' || command.op === 'unsubscribeOrderBook'
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
    if (this.disposed) return;
    const frame = this.engine.takeFrame(this.transport.now(), marketOnly);
    if (frame) this.handlers.onFrame?.(frame.buffer);
  }

  /** The whole current state as one frame (an ArrayBuffer), for a consumer
   * that starts late or skipped frames; later flushes apply on top of it. */
  snapshot() {
    if (this.disposed) throw new Error('engine disposed');
    return this.engine.snapshotFrame(this.transport.now()).buffer;
  }

  accountId() {
    if (this.disposed) throw new Error('engine disposed');
    return this.engine.accountId?.();
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    for (const controller of this.requests.values()) controller.abort();
    this.requests.clear();
    for (const pending of this.commands.values()) pending.reject(new Error('engine disposed'));
    this.commands.clear();
    for (const socket of this.sockets.values()) socket.close();
    for (const handle of this.timers.values()) this.transport.clearTimeout(handle);
    this.sockets.clear();
    this.wsScratch = undefined;
    this.timers.clear();
    this.transport.dispose?.();
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
          if (this.disposed) return;
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

  async request(action) {
    const id = action.request;
    const controller = new AbortController();
    this.requests.set(id, controller);
    try {
      const response = await this.transport.request({
        method: action.method,
        ...(action.route ? { route: action.route } : {}),
        ...(action.timeoutMs !== undefined ? { timeoutMs: action.timeoutMs } : {}),
        url: action.url,
        headers: Object.fromEntries(action.headers),
        body: action.body,
        signal: controller.signal,
      });
      if (!this.disposed) await this.receive(id, response);
    } catch (error) {
      if (!this.disposed) this.run(this.engine.httpFailed(id, String(error?.message ?? error), this.transport.now()));
    } finally {
      this.requests.delete(id);
    }
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
        const wsMessageBytes = typeof engine.wsMessageBytes === 'function' ? engine.wsMessageBytes : undefined;
        const wsMessage = typeof engine.wsMessageActions === 'function' ? engine.wsMessageActions : engine.wsMessage;
        if (action.headers?.length && !transport.webSocketHeaders) {
          this.handlers.onLog?.('warning', 'WebSocket upgrade headers unavailable; continuing REST reconciliation');
          transport.setTimeout(() => { if (!this.disposed) this.run(engine.wsClose(id, 1006, transport.now())); }, 0);
          break;
        }
        let connecting = true;
        let closed = false;
        const socket = transport.connect(action.url, {
          binary: action.binary === true,
          channel: action.channel,
          context: action.context,
          headers: action.headers && Object.fromEntries(action.headers),
          onOpen: () => { if (!this.disposed && !closed) this.run(engine.wsOpen(id, transport.now())); },
          onMessage: (text) => {
            if (this.disposed || closed) return;
            if (!wsMessageBytes) {
              this.run(wsMessage.call(engine, id, text, transport.now()));
              return;
            }
            // UTF-8 needs at most three bytes per UTF-16 unit, including lone
            // surrogates. Keep one small buffer; large or reentrant messages own theirs.
            const scratch = text.length <= 1365 && !this.wsScratchBusy;
            if (scratch) this.wsScratchBusy = true;
            let actions;
            try {
              const encoder = this.wsEncoder ??= new TextEncoder();
              let bytes;
              if (scratch && typeof encoder.encodeInto === 'function') {
                const buffer = this.wsScratch ??= new Uint8Array(4096);
                const { read, written } = encoder.encodeInto(text, buffer);
                bytes = read === text.length ? buffer.subarray(0, written) : encoder.encode(text);
              } else {
                bytes = encoder.encode(text);
              }
              actions = wsMessageBytes.call(engine, id, bytes, transport.now());
            } finally {
              if (scratch) this.wsScratchBusy = false;
            }
            this.run(actions);
          },
          onBinary: (data) => {
            if (this.disposed || closed || (!connecting && !this.sockets.has(id))) return;
            if (engine.wsBinary) this.run(engine.wsBinary(id, data, transport.now()));
            else {
              closed = true;
              const active = this.sockets.get(id);
              this.sockets.delete(id);
              active?.close();
              this.run(engine.wsClose(id, 1003, transport.now()));
            }
          },
          onClose: (code) => {
            if (this.disposed || closed || (!connecting && !this.sockets.has(id))) return;
            closed = true;
            this.sockets.delete(id);
            this.run(engine.wsClose(id, code ?? 1006, transport.now()));
          },
        });
        connecting = false;
        if (closed || this.disposed) socket.close();
        else this.sockets.set(id, socket);
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
        void this.request(action);
        break;
      }
      case 'setTimer': {
        const id = action.timer;
        const handle = transport.setTimeout(() => {
          if (this.disposed) return;
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
      case 'deactivate':
        this.handlers.onDeactivate?.();
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

/** Deliver compressed binary messages and closure in receive order on every JS host. */
export const bindSocketMessages = (socket, { binary = false, onBinary, onMessage, onClose, onError }) => {
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
      if (binary && blob.size > 16 * 1024 * 1024) throw new Error('websocket message exceeds 16 MiB');
      const bytes = new Uint8Array(await blob.arrayBuffer());
      if (disposed) return;
      if (binary) {
        if (bytes.byteLength > 16 * 1024 * 1024) throw new Error('websocket message exceeds 16 MiB');
        onBinary?.(bytes);
        return;
      }
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
export const socketMessages = (onMessage, onError, binary = false) => {
  let receive;
  bindSocketMessages({
    addEventListener(type, listener) { if (type === 'message') receive = listener; },
    close() {},
  }, { binary, onBinary: onMessage, onMessage, onError, onClose() {} });
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
  WebSocket: webSocketHeaders ? Socket : undefined,
  webSocketHeaders,
  now: () => Date.now(),
  connect(url, { headers = {}, binary = false, onOpen, onMessage, onBinary, onClose }) {
    if (Object.keys(headers).length && !webSocketHeaders) throw new Error('WebSocket upgrade headers unavailable');
    const ws = webSocketHeaders ? new Socket(url, { headers: { 'User-Agent': 'Tealstreet', ...headers } }) : new Socket(url);
    let closed = false;
    let reported = false;
    const reportClose = (code) => { if (!reported) { reported = true; closed = true; onClose(code); } };
    ws.addEventListener('open', () => { if (!closed) onOpen(); });
    const dispose = bindSocketMessages(ws, {
      binary,
      onBinary: (data) => { if (!closed) onBinary?.(data); },
      onMessage: (text) => { if (!closed) onMessage(text); },
      onClose: reportClose,
    });
    // Local close disposes queued decoding, but still acknowledges closure once.
    ws.addEventListener('error', () => { if (closed) reportClose(1006); });
    ws.addEventListener('close', (event) => { if (closed) reportClose(event.code); });
    return { send: (text) => ws.send(text), close: () => { closed = true; dispose(); ws.close(); } };
  },
  async request({ method, url, headers, body, signal, route, timeoutMs }) {
    if (route === 'publicCors') throw new Error('Public CORS request routing is not configured');
    const deadline = AbortSignal.timeout(Math.max(1,timeoutMs ?? 20_000));
    const response = await fetch(url, { method, headers, body, signal: signal ? AbortSignal.any([signal, deadline]) : deadline });
    const lines = [];
    response.headers.forEach((value, name) => lines.push(`${name}: ${value}`));
    return { status: response.status, headers: lines.join('\n'), body: chunks(response.body) };
  },
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (handle) => clearTimeout(handle),
};
};
