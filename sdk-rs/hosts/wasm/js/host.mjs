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

const CHUNK_BYTES = 64 * 1024;
const BATCH_BYTES = 1024 * 1024;
const BATCH_MESSAGES = 256;
const RETAINED_SCRATCH_BYTES = 64 * 1024;
const encoder = new TextEncoder();

export class EngineHost {
  constructor(engine, transport, handlers = {}, { batchMessages = true } = {}) {
    this.engine = engine;
    this.transport = transport;
    this.handlers = handlers;
    this.sockets = new Map();
    this.timers = new Map();
    this.commands = new Map();
    this.nextCommand = 1;
    this.batchMessages = batchMessages;
    this.messages = [];
    this.messageBytes = 0;
    this.batchBuffer = new Uint8Array(0);
    this.batchTimer = undefined;
    this.disposed = false;
  }

  start() {
    if (this.disposed) return;
    this.run(this.engine.start(this.transport.now()));
  }

  /** Send a command; resolves with its result for commands that report one. */
  command(command) {
    if (this.disposed) return Promise.reject(new Error('engine disposed'));
    this.drainMessages();
    if (this.disposed) return Promise.reject(new Error('engine disposed'));
    const reply = command.op === 'subscribeOrderBook' || command.op === 'unsubscribeOrderBook'
      || command.op === 'subscribeCandles' || command.op === 'unsubscribeCandles'
      ? undefined
      : this.nextCommand++;
    let actions;
    try {
      actions = this.engine.command(JSON.stringify(reply === undefined ? command : { ...command, command: reply }), this.transport.now());
    } catch (error) {
      return Promise.reject(error);
    }
    if (this.disposed) return Promise.reject(new Error('engine disposed'));
    const promise = reply === undefined
      ? Promise.resolve()
      : new Promise((resolve, reject) => this.commands.set(reply, { resolve, reject }));
    try {
      this.run(actions);
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
    this.drainMessages();
    if (this.disposed) return;
    const frame = this.engine.takeFrame(this.transport.now(), marketOnly);
    if (frame) this.handlers.onFrame?.(frame.buffer);
  }

  /** The whole current state as one frame (an ArrayBuffer), for a consumer
   * that starts late or skipped frames; later flushes apply on top of it. */
  snapshot() {
    if (this.disposed) throw new Error('engine disposed');
    this.drainMessages();
    if (this.disposed) throw new Error('engine disposed');
    return this.engine.snapshotFrame(this.transport.now()).buffer;
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.transport.clearTimeout(this.batchTimer);
    this.batchTimer = undefined;
    this.messages.length = 0;
    this.messageBytes = 0;
    this.batchBuffer = new Uint8Array(0);
    for (const socket of this.sockets.values()) socket.close();
    for (const handle of this.timers.values()) this.transport.clearTimeout(handle);
    this.sockets.clear();
    this.timers.clear();
    for (const { reject } of this.commands.values()) reject(new Error('engine disposed'));
    this.commands.clear();
  }

  /** Feed a response to the engine in chunks of at most `CHUNK_BYTES`. */
  async receive(id, { status, headers = '', body }) {
    if (this.disposed) return;
    const { engine, transport } = this;
    this.drainMessages();
    if (this.disposed) return;
    this.run(engine.httpStart(id, status, headers, transport.now()));
    try {
      if (body instanceof Uint8Array) {
        for (let at = 0; at < body.length; at += CHUNK_BYTES) {
          this.drainMessages();
          if (this.disposed) return;
          this.run(engine.httpChunk(id, body.subarray(at, at + CHUNK_BYTES), transport.now()));
        }
      } else {
        for await (const chunk of body) {
          if (this.disposed) return;
          this.drainMessages();
          if (this.disposed) return;
          this.run(engine.httpChunk(id, chunk, transport.now()));
        }
      }
    } catch (error) {
      if (this.disposed) return;
      this.drainMessages();
      if (this.disposed) return;
      this.run(engine.httpFailed(id, String(error?.message ?? error), transport.now()));
      return;
    }
    if (this.disposed) return;
    this.drainMessages();
    if (this.disposed) return;
    this.run(engine.httpEnd(id, transport.now()));
  }

  message(socket, text) {
    if (this.disposed) return;
    const now = this.transport.now();
    if (!this.batchMessages) {
      this.run(this.engine.wsMessage(socket, text, now));
      return;
    }
    // UTF-8 needs at most three bytes per JS code unit. Bound pending memory
    // even when a busy socket prevents the scheduled task from running.
    const bytes = 16 + text.length * 3;
    this.messages.push(socket, now, text);
    this.messageBytes += bytes;
    if (this.messages.length >= BATCH_MESSAGES * 3 || this.messageBytes >= BATCH_BYTES) {
      this.drainMessages();
    } else if (this.batchTimer === undefined) {
      this.batchTimer = this.transport.setTimeout(() => {
        this.batchTimer = undefined;
        this.drainMessages();
      }, 0);
    }
  }

  /** Deliver queued arrivals before any later event can overtake them. */
  drainMessages() {
    if (this.disposed || !this.messages.length) return;
    this.transport.clearTimeout(this.batchTimer);
    this.batchTimer = undefined;
    const messages = this.messages;
    this.messages = [];
    const capacity = this.messageBytes;
    this.messageBytes = 0;
    if (this.batchBuffer.length < capacity) this.batchBuffer = new Uint8Array(capacity);
    const buffer = this.batchBuffer;
    const view = new DataView(buffer.buffer);
    let at = 0;
    for (let i = 0; i < messages.length; i += 3) {
      const { written } = encoder.encodeInto(messages[i + 2], buffer.subarray(at + 16));
      view.setUint32(at, messages[i], true);
      view.setUint32(at + 4, written, true);
      view.setFloat64(at + 8, messages[i + 1], true);
      at += 16 + written;
    }
    const actions = this.engine.wsMessages(buffer.subarray(0, at));
    // Bursts and large individual messages must not grow retained scratch.
    if (buffer.length > RETAINED_SCRATCH_BYTES) this.batchBuffer = new Uint8Array(0);
    this.run(actions);
  }

  run(actionsJson) {
    if (this.disposed || !actionsJson) return;
    for (const action of JSON.parse(actionsJson)) {
      if (this.disposed) break;
      this.execute(action);
    }
  }

  execute(action) {
    if (this.disposed) return;
    const { engine, transport } = this;
    switch (action.op) {
      case 'wsConnect': {
        const id = action.socket;
        const socket = transport.connect(action.url, {
          onOpen: () => {
            if (this.disposed) return;
            this.drainMessages();
            if (this.disposed) return;
            this.run(engine.wsOpen(id, transport.now()));
          },
          onMessage: (text) => this.message(id, text),
          onClose: (code) => {
            if (this.disposed) return;
            this.drainMessages();
            if (this.disposed) return;
            this.sockets.delete(id);
            this.run(engine.wsClose(id, code ?? 1006, transport.now()));
          },
        });
        if (this.disposed) socket.close();
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
            (error) => {
              if (this.disposed) return;
              this.drainMessages();
              if (this.disposed) return;
              this.run(engine.httpFailed(id, String(error?.message ?? error), transport.now()));
            },
          );
        break;
      }
      case 'setTimer': {
        const id = action.timer;
        transport.clearTimeout(this.timers.get(id));
        const handle = transport.setTimeout(() => {
          if (this.disposed) return;
          this.drainMessages();
          if (this.disposed) return;
          this.timers.delete(id);
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
        if (action.error !== undefined) pending.reject(new Error(action.error));
        else {
          try {
            pending.resolve(action.result === undefined ? undefined : JSON.parse(action.result));
          } catch (error) {
            pending.reject(error);
          }
        }
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

/** Transport over the runtime's own WebSocket, fetch and timers. */
export const nativeTransport = () => ({
  now: () => Date.now(),
  connect(url, { onOpen, onMessage, onClose }) {
    const ws = new WebSocket(url);
    ws.addEventListener('open', () => onOpen());
    ws.addEventListener('message', (event) => {
      if (typeof event.data === 'string') onMessage(event.data);
    });
    ws.addEventListener('close', (event) => onClose(event.code));
    return { send: (text) => ws.send(text), close: () => ws.close() };
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
