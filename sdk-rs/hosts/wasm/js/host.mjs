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
const BATCH_BYTES = 64 * 1024;
const BATCH_MESSAGES = 256;
const RETAINED_SCRATCH_BYTES = 16 * 1024;
const encoder = new TextEncoder();

export class EngineHost {
  constructor(engine, transport, handlers = {}, { batchMessages = true } = {}) {
    this.engine = engine;
    this.transport = transport;
    this.handlers = handlers;
    this.sockets = new Map();
    this.timers = new Map();
    this.commands = new Map();
    this.requests = new Map();
    this.signings = new Map();
    this.nextCommand = 1;
    this.batchMessages = batchMessages;
    this.messages = [];
    this.messageBytes = 0;
    this.hasBinaryMessages = false;
    this.batchBuffer = new Uint8Array(0);
    this.batchTimer = undefined;
    this.wsEncoder = undefined;
    this.wsScratch = undefined;
    this.wsScratchBusy = false;
    this.socketFallbacks = new Set();
    this.disposed = false;
    this.ingress = [];
    this.ingressDraining = false;
    this.ingressDelivery = false;
    this.ingressTimer = undefined;
    this.queuedCommands = new Set();
    this.queuedHttp = new Set();
  }

  /** Reserve an asynchronous raw arrival before decoding can yield. */
  reserveIngress() {
    if (this.disposed) return () => {};
    const entry = { ready: false, deliver: undefined, reserved: true };
    this.ingress.push(entry);
    return (deliver) => {
      if (entry.ready || this.disposed) return;
      entry.ready = true;
      entry.deliver = deliver;
      this.drainIngress();
    };
  }

  ordered(deliver) {
    if (this.ingressDelivery) {
      this.ingressDelivery = false;
      return deliver();
    }
    if (this.ingress.length || this.ingressDraining) {
      this.ingress.push({ ready: true, deliver });
      this.drainIngress();
    } else return deliver();
  }

  drainIngress() {
    if (this.disposed || this.ingressDraining) return;
    this.ingressDraining = true;
    // Reentrant callbacks belong to the next finite prefix.
    const count = this.ingress.length;
    try {
      for (let i = 0; i < count && !this.disposed && this.ingress[0]?.ready; i++) {
        const entry = this.ingress.shift();
        this.ingressDelivery = entry.reserved === true;
        try { entry.deliver?.(); }
        finally { this.ingressDelivery = false; }
      }
    } finally {
      this.ingressDraining = false;
      if (!this.disposed && this.ingress[0]?.ready && this.ingressTimer === undefined) {
        this.ingressTimer = this.transport.setTimeout(() => {
          this.ingressTimer = undefined;
          this.drainIngress();
        }, 0);
      }
    }
  }

  start() {
    if (this.disposed) return;
    const now = this.transport.now();
    this.ordered(() => { if (!this.disposed) this.run(this.engine.start(now)); });
  }

  /** Send a command; resolves with its result for commands that report one. */
  command(command) {
    if (this.disposed) return Promise.reject(new Error('engine disposed'));
    const reply = command.op === 'publicFeedUpdate' || command.op === 'setTickerSymbols' || command.op === 'setTickInterval' || command.op === 'subscribeOrderBook' || command.op === 'unsubscribeOrderBook'
      || command.op === 'subscribeCandles' || command.op === 'unsubscribeCandles'
      ? undefined
      : this.nextCommand++;
    let json;
    try { json = JSON.stringify(reply === undefined ? command : { ...command, command: reply }); }
    catch (error) { return Promise.reject(error); }
    const now = this.transport.now();
    return new Promise((resolve, reject) => {
      this.queuedCommands.add(reject);
      this.ordered(() => {
        this.queuedCommands.delete(reject);
        if (this.disposed) { reject(new Error('engine disposed')); return; }
        try { resolve(this.dispatchCommand(json, reply, now)); }
        catch (error) { reject(error); }
      });
    });
  }

  dispatchCommand(json, reply, now) {
    this.drainMessages();
    if (this.disposed) return Promise.reject(new Error('engine disposed'));
    let actions;
    try {
      actions = this.engine.command(json, now);
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
    const now = this.transport.now();
    this.ordered(() => this.dispatchFlush(marketOnly, now));
  }

  dispatchFlush(marketOnly, now) {
    this.drainMessages();
    if (this.disposed) return;
    const frame = this.engine.takeFrame(now, marketOnly);
    if (frame) this.handlers.onFrame?.(frame.buffer);
  }

  /** The whole current state as one frame (an ArrayBuffer), for a consumer
   * that starts late or skipped frames; later flushes apply on top of it. */
  snapshot() {
    if (this.disposed) throw new Error('engine disposed');
    this.drainIngress();
    this.drainMessages();
    if (this.disposed) throw new Error('engine disposed');
    return this.engine.snapshotFrame(this.transport.now()).buffer;
  }

  accountId() {
    if (this.disposed) throw new Error('engine disposed');
    this.drainIngress();
    this.drainMessages();
    if (this.disposed) throw new Error('engine disposed');
    return this.engine.accountId?.();
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    if (this.ingressTimer !== undefined) this.transport.clearTimeout(this.ingressTimer);
    this.ingressTimer = undefined;
    this.ingress.length = 0;
    for (const reject of this.queuedCommands) reject(new Error('engine disposed'));
    this.queuedCommands.clear();
    for (const resolve of this.queuedHttp) resolve();
    this.queuedHttp.clear();
    for (const controller of this.requests.values()) controller.abort();
    this.requests.clear();
    if (this.batchTimer !== undefined) this.transport.clearTimeout(this.batchTimer);
    this.batchTimer = undefined;
    this.messages.length = 0;
    this.messageBytes = 0;
    this.hasBinaryMessages = false;
    this.batchBuffer = new Uint8Array(0);
    this.wsScratch = undefined;
    for (const handle of this.socketFallbacks) this.transport.clearTimeout(handle);
    this.socketFallbacks.clear();
    for (const pending of this.signings.values()) {
      pending.controller.abort();
      this.transport.clearTimeout(pending.timer);
    }
    this.signings.clear();

    for (const socket of this.sockets.values()) socket.close();
    for (const handle of this.timers.values()) this.transport.clearTimeout(handle);
    this.sockets.clear();
    this.timers.clear();
    for (const { reject } of this.commands.values()) reject(new Error('engine disposed'));
    this.commands.clear();
    this.transport.dispose?.();
  }

  /** Feed a response to the engine in chunks of at most `CHUNK_BYTES`. */
  async receive(id, { status, headers = '', body }) {
    if (this.disposed) return;
    const { engine } = this;
    // A caller may reuse a whole-body buffer while its headers wait in FIFO.
    if (body instanceof Uint8Array) {
      if (this.ingress.length || this.ingressDraining) body = Uint8Array.from(body);
      const bytes = body;
      const complete = this.httpEvent((now) => {
        this.run(engine.httpStart(id, status, headers, now));
        if (this.disposed) return;
        try {
          for (let at = 0; at < bytes.length && !this.disposed; at += CHUNK_BYTES)
            this.run(engine.httpChunk(id, bytes.subarray(at, at + CHUNK_BYTES), now));
        } catch (error) {
          if (!this.disposed) this.run(engine.httpFailed(id, String(error?.message ?? error), now));
          return;
        }
        if (!this.disposed) this.run(engine.httpEnd(id, now));
      });
      if (complete) await complete;
      return;
    }
    const started = this.httpEvent((now) => engine.httpStart(id, status, headers, now));
    if (started) await started;
    if (this.disposed) return;
    try {
      for await (const chunk of body) {
        if (this.disposed) return;
        const bytes = this.ingress.length || this.ingressDraining ? Uint8Array.from(chunk) : chunk;
        const sent = this.httpEvent((now) => {
          for (let at = 0; at < bytes.length && !this.disposed; at += CHUNK_BYTES)
            this.run(engine.httpChunk(id, bytes.subarray(at, at + CHUNK_BYTES), now));
        });
        if (sent) await sent;
        if (this.disposed) return;
      }
    } catch (error) {
      if (this.disposed) return;
      await this.httpEvent((now) => engine.httpFailed(id, String(error?.message ?? error), now));
      return;
    }
    if (this.disposed) return;
    await this.httpEvent((now) => engine.httpEnd(id, now));
  }

  httpEvent(actions) {
    const now = this.transport.now();
    if (this.disposed) return;
    const deliver = () => {
      this.drainMessages();
      if (!this.disposed) this.run(actions(now));
    };
    if (!this.ingress.length && !this.ingressDraining) return deliver();
    return new Promise((resolve, reject) => {
      this.queuedHttp.add(resolve);
      this.ordered(() => {
        this.queuedHttp.delete(resolve);
        try {
          deliver();
          resolve();
        } catch (error) { reject(error); }
      });
    });
  }

  singleMessage(socket, text, now, preferBytes = false) {
    const { engine } = this;
    if (preferBytes && typeof engine.wsMessageBytes === 'function') {
      const scratch = text.length <= 1365 && !this.wsScratchBusy;
      if (scratch) this.wsScratchBusy = true;
      try {
        const utf8 = this.wsEncoder ??= new TextEncoder();
        let bytes;
        if (scratch && typeof utf8.encodeInto === 'function') {
          const buffer = this.wsScratch ??= new Uint8Array(4096);
          const { read, written } = utf8.encodeInto(text, buffer);
          bytes = read === text.length ? buffer.subarray(0, written) : utf8.encode(text);
        } else bytes = utf8.encode(text);
        return engine.wsMessageBytes(socket, bytes, now);
      } finally {
        if (scratch) this.wsScratchBusy = false;
      }
    }
    return (engine.wsMessageActions ?? engine.wsMessage).call(engine, socket, text, now);
  }

  message(socket, data, receivedAt) {
    if (this.disposed) return;
    const now = receivedAt ?? this.transport.now();
    if (this.ingressDelivery) this.ingressDelivery = false;
    else if (this.ingress.length || this.ingressDraining) {
      const owned = data instanceof Uint8Array ? Uint8Array.from(data) : data;
      this.ordered(() => this.deliverMessage(socket, owned, now));
      return;
    }
    this.deliverMessage(socket, data, now);
  }

  deliverMessage(socket, data, now) {
    if (!this.batchMessages) {
      this.run(typeof data === 'string' ? this.singleMessage(socket, data, now, true)
        : this.engine.wsBinary(socket, data, now));
      return;
    }
    // UTF-8 needs at most three bytes per JS code unit. Bound pending memory
    // even when a busy socket prevents the scheduled task from running.
    const binary = data instanceof Uint8Array;
    const bytes = 20 + (binary ? data.byteLength : data.length * 3);
    // The callback may reuse its mutable buffer after returning. Retain an
    // arrival snapshot; the Rust batch decoder still borrows its packed slice.
    this.messages.push(socket, now, binary ? Uint8Array.from(data) : data);
    if (binary) this.hasBinaryMessages = true;
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
    if (this.batchTimer !== undefined) this.transport.clearTimeout(this.batchTimer);
    this.batchTimer = undefined;
    const messages = this.messages;
    const mixed = this.hasBinaryMessages;
    const singletonClock = Number.isFinite(messages[1]);
    this.messages = [];
    this.messageBytes = 0;
    this.hasBinaryMessages = false;
    // Validate the entire captured prefix before any partition changes state.
    for (let i = 1; i < messages.length; i += 3) {
      messages[i] = +messages[i];
      if (!Number.isFinite(messages[i])) throw new Error('invalid websocket batch arrival time');
    }
    // A singleton still waits for this scheduled drain or event barrier. Avoid
    // packing it when the single-message entry point has identical input types;
    // unusual clocks/payloads retain the batch encoder's validation/coercion.
    if (messages.length === 3 && typeof messages[2] === 'string' && singletonClock) {
      this.run(this.singleMessage(messages[0], messages[2], messages[1], true));
      return;
    }
    if (messages.length === 3 && messages[2] instanceof Uint8Array && singletonClock) {
      this.run(this.engine.wsBinary(messages[0], messages[2], messages[1]));
      return;
    }
    const header = mixed ? 20 : 16;
    const pendingActions = [];
    // Older binary-capable engines lack the mixed batch ABI. Preserve their
    // ordered delivery and deferred actions without pretending they batch.
    if (mixed && typeof this.engine.wsEvents !== 'function') {
      for (let i = 0; i < messages.length && !this.disposed; i += 3) {
        const data = messages[i + 2];
        const actions = data instanceof Uint8Array ? this.engine.wsBinary(messages[i], data, messages[i + 1])
          : this.singleMessage(messages[i], data, messages[i + 1]);
        if (actions) pendingActions.push(actions);
      }
      for (const actions of pendingActions) this.run(actions);
      return;
    }
    for (let start = 0; start < messages.length && !this.disposed;) {
      let end = start;
      let capacity = 0;
      while (end < messages.length) {
        const data = messages[end + 2];
        const bytes = header + (data instanceof Uint8Array ? data.byteLength : data.length * 3);
        if (end > start && capacity + bytes > BATCH_BYTES) break;
        capacity += bytes;
        end += 3;
      }
      if (this.batchBuffer.length < capacity) this.batchBuffer = new Uint8Array(capacity);
      const buffer = this.batchBuffer;
      const view = new DataView(buffer.buffer);
      let at = 0;
      for (let i = start; i < end; i += 3) {
        const data = messages[i + 2];
        const binary = data instanceof Uint8Array;
        let written;
        if (binary) { buffer.set(data, at + header); written = data.byteLength; }
        else ({ written } = encoder.encodeInto(data, buffer.subarray(at + header)));
        view.setUint32(at, messages[i], true);
        view.setUint32(at + 4, written, true);
        view.setFloat64(at + 8, messages[i + 1], true);
        if (mixed) view.setUint32(at + 16, Number(binary), true);
        at += header + written;
      }
      const actions = mixed ? this.engine.wsEvents(buffer.subarray(0, at))
        : this.engine.wsMessages(buffer.subarray(0, at));
      if (actions) pendingActions.push(actions);
      // Bursts and large individual messages must not grow retained scratch.
      if (buffer.length > RETAINED_SCRATCH_BYTES) this.batchBuffer = new Uint8Array(0);
      start = end;
    }
    // All captured arrivals precede transport callbacks from any of their
    // actions, even when an oversized last arrival needs its own crossing.
    for (const actions of pendingActions) this.run(actions);
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
      if (this.disposed) return;
      await this.httpEvent((now) => this.engine.httpFailed(id, String(error?.message ?? error), now));
    } finally {
      this.requests.delete(id);
    }
  }

  signPayload(action) {
    const id = action.request;
    const controller = new AbortController();
    const finish = (error, result) => {
      const pending = this.signings.get(id);
      if (!pending || pending.controller !== controller || this.disposed) return;
      this.signings.delete(id);
      this.transport.clearTimeout(pending.timer);
      if (error) controller.abort();
      this.ordered(() => {
        this.drainMessages();
        if (this.disposed) return;
        // Signing and its HTTP continuation share a wall-clock deadline.
        const now = this.transport.now();
        this.run(error
          ? this.engine.payloadSignFailed(id, String(error?.message ?? error), now)
          : this.engine.payloadSigned(id, result.signature, result.headers, now));
      });
    };
    const timer = this.transport.setTimeout(() => finish(new Error('Payload signing timed out')), action.timeoutMs);
    this.signings.set(id, { controller, timer });
    Promise.resolve().then(() => {
      if (this.disposed || controller.signal.aborted) return;
      if (!this.transport.signPayload) throw new Error('Payload signing is not configured');
      return this.transport.signPayload(action.payload, { signal: controller.signal });
    }).then((result) => {
      if (this.disposed || controller.signal.aborted) return;
      const signature = result?.signature;
      const record = result?.headers;
      if (typeof signature !== 'string'
        || (record !== undefined && (!record || typeof record !== 'object' || Array.isArray(record)))) {
        finish(new Error('Invalid payload signer result'));
        return;
      }
      const headers = Object.entries(record ?? {});
      if (headers.some(([name, value]) => !/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name)
        || typeof value !== 'string' || /[\r\n]/.test(value))) {
        finish(new Error('Invalid payload signer header'));
        return;
      }
      finish(undefined, { signature, headers: headers.map(([name, value]) => `${name}:${value}`).join('\n') });
    }).catch((error) => finish(new Error(String(error?.message ?? error))));
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
      case 'signPayload':
        this.signPayload(action);
        break;
      case 'wsConnect': {
        const id = action.socket;
        if (action.headers?.length && !transport.webSocketHeaders) {
          this.handlers.onLog?.('warning', 'WebSocket upgrade headers unavailable; continuing REST reconciliation');
          if (this.disposed) break;
          let handle;
          let fired = false;
          handle = transport.setTimeout(() => {
            fired = true;
            this.socketFallbacks.delete(handle);
            if (this.disposed) return;
            const now = transport.now();
            this.ordered(() => {
              this.drainMessages();
              if (!this.disposed) this.run(engine.wsClose(id, 1006, now));
            });
          }, 0);
          if (!fired && !this.disposed) this.socketFallbacks.add(handle);
          else if (this.disposed) transport.clearTimeout(handle);
          break;
        }
        const socket = transport.connect(action.url, {
          binary: action.binary === true,
          channel: action.channel,
          context: action.context,
          headers: action.headers && Object.fromEntries(action.headers),
          onIngress: () => this.reserveIngress(),
          onOpen: () => {
            if (this.disposed) return;
            const now = transport.now();
            this.ordered(() => {
              this.drainMessages();
              if (!this.disposed) this.run(engine.wsOpen(id, now));
            });
          },
          onMessage: (text, receivedAt) => this.message(id, text, receivedAt),
          onBinary: (data, receivedAt) => {
            if (this.disposed || !this.sockets.has(id)) return;
            if (typeof engine.wsBinary === 'function') this.message(id, data, receivedAt);
            else {
              const now = receivedAt ?? transport.now();
              this.ordered(() => {
                this.drainMessages();
                if (this.disposed || !this.sockets.has(id)) return;
                const active = this.sockets.get(id);
                this.sockets.delete(id);
                active?.close();
                this.run(engine.wsClose(id, 1003, now));
              });
            }
          },
          onClose: (code, receivedAt) => {
            if (this.disposed || !this.sockets.has(id)) return;
            const now = receivedAt ?? transport.now();
            this.ordered(() => {
              this.drainMessages();
              if (this.disposed || !this.sockets.has(id)) return;
              this.sockets.delete(id);
              this.run(engine.wsClose(id, code ?? 1006, now));
            });
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
        void this.request(action);
        break;
      }
      case 'setTimer': {
        const id = action.timer;
        transport.clearTimeout(this.timers.get(id));
        const handle = transport.setTimeout(() => {
          if (this.disposed) return;
          const now = transport.now();
          this.ordered(() => {
            this.drainMessages();
            if (this.disposed) return;
            this.timers.delete(id);
            this.run(engine.timer(id, now));
          });
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

/** Decode binary feeds in arrival order while leaving text feeds synchronous. */
export const socketMessages = (onMessage, onError, onClose = () => {}, binary = false, onIngress) => {
  // Retain the incoming three-argument binary form as well as FIFO close hooks.
  if (typeof onClose === 'boolean') { binary = onClose; onClose = () => {}; }
  let pending;
  let accepting = true;
  let failed = false;
  let cancelled = false;
  let closeQueued = false;
  const reservations = new Set();
  const reserve = () => {
    const settle = onIngress?.();
    if (settle) reservations.add(settle);
    return (deliver) => {
      if (settle) { reservations.delete(settle); settle(deliver); }
      else deliver?.();
    };
  };
  const enqueue = (task) => {
    const next = (pending ?? Promise.resolve()).then(task, task);
    pending = next;
    const clear = () => { if (pending === next) pending = undefined; };
    next.then(clear, clear);
    return next;
  };
  const decode = async (data) => {
    if (typeof data === 'string') return data;
    const bytes = data instanceof Blob ? new Uint8Array(await data.arrayBuffer())
      : data instanceof ArrayBuffer ? new Uint8Array(data)
      : new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
    if (binary) return bytes;
    const format = bytes[0] === 0x1f && bytes[1] === 0x8b ? 'gzip'
      : bytes[0] === 0x78 ? 'deflate' : undefined;
    if (!format) return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream(format));
    return new TextDecoder('utf-8', { fatal: true }).decode(await new Response(stream).arrayBuffer());
  };
  const receive = (data, receivedAt = Date.now()) => {
    if (!accepting || cancelled) return;
    if (typeof data === 'string' && !pending) return onMessage(data, receivedAt);
    const commit = reserve();
    // Views/ArrayBuffers can be reused as soon as the raw callback returns.
    if (data instanceof ArrayBuffer) data = data.slice(0);
    else if (ArrayBuffer.isView(data)) data = Uint8Array.from(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
    return enqueue(async () => {
      if (cancelled || failed) { commit(); return; }
      let text;
      try {
        text = await decode(data);
      } catch (error) {
        if (!cancelled && !failed) {
          failed = true;
          accepting = false;
          commit(() => { if (!cancelled) onError(error, receivedAt); });
        } else commit();
        return;
      }
      commit(() => { if (!cancelled) onMessage(text, receivedAt); });
    });
  };
  receive.close = (code, receivedAt = Date.now()) => {
    if (closeQueued || cancelled) return;
    closeQueued = true;
    accepting = false;
    const commit = reserve();
    const report = () => { if (!cancelled) onClose(code, receivedAt); };
    return pending ? enqueue(() => commit(report)) : commit(report);
  };
  receive.cancel = () => {
    cancelled = true;
    accepting = false;
    for (const settle of reservations) settle();
    reservations.clear();
  };
  return receive;
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
  connect(url, { headers = {}, binary = false, onOpen, onMessage, onBinary, onClose, onIngress }) {
    if (Object.keys(headers).length && !webSocketHeaders) throw new Error('WebSocket upgrade headers unavailable');
    const ws = webSocketHeaders ? new Socket(url, { headers: { 'User-Agent': 'Tealstreet', ...headers } }) : new Socket(url);
    let reported = false;
    let closing = false;
    let localClose = false;
    const reportClose = (code, receivedAt) => {
      if (!reported) { reported = true; onClose(code, receivedAt); }
    };
    const receive = socketMessages((data, receivedAt) => {
      if (typeof data === 'string') onMessage(data, receivedAt);
      else onBinary?.(data, receivedAt);
    }, (_error, receivedAt) => {
      closing = true;
      receive.cancel();
      reportClose(1007, receivedAt);
      ws.close();
    }, reportClose, binary, onIngress);
    const close = (code, receivedAt) => {
      closing = true;
      if (localClose) reportClose(code, receivedAt);
      else receive.close(code, receivedAt);
    };
    ws.addEventListener('error', () => close(1006, Date.now()));
    ws.addEventListener('open', () => { if (!closing) onOpen(); });
    ws.addEventListener('message', (event) => receive(event.data, Date.now()));
    ws.addEventListener('close', (event) => close(event.code, Date.now()));
    return {
      send: (text) => ws.send(text),
      close: () => { localClose = true; closing = true; receive.cancel(); ws.close(); },
    };
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
