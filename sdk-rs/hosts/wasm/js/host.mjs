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
export const DEFAULT_COMMAND_TIMEOUT_MS = 120_000;
const commandTimeout = (value) => {
  if (!Number.isSafeInteger(value) || value <= 0 || value > 2_147_483_647) {
    throw new RangeError('Command timeout must be a positive safe integer <= 2147483647');
  }
  return value;
};
const WRITE_COMMANDS = new Set(['placeOrders', 'cancelOrders', 'cancelOrdersWithSymbols', 'amendOrder',
  'amendOrderWithSymbol', 'cancelSymbolOrders', 'setLeverage', 'setMarginType', 'setPositionMode', 'setStpMode']);
class UnknownEngineActionError extends Error {}
class CommandDiagnosticCallbackError extends Error {
  constructor(cause) { super('Command diagnostic callback failed', { cause }); }
}
class CommandProgressCallbackError extends Error {
  constructor(cause) { super('Command progress callback failed', { cause }); }
}
class AccountAuthCallbackError extends Error {
  constructor(cause) { super('Account authentication callback failed', { cause }); }
}
let nextReceiptSession = 0;

function receiptSession() {
  const sequence = ++nextReceiptSession;
  try {
    if (typeof globalThis.crypto?.randomUUID === 'function') return `${globalThis.crypto.randomUUID()}:${sequence}`;
  } catch { /* Correlation must also work in native hosts without Web Crypto. */ }
  const bytes = new Uint8Array(16);
  try {
    if (typeof globalThis.crypto?.getRandomValues !== 'function') throw new Error('Web Crypto unavailable');
    globalThis.crypto.getRandomValues(bytes);
  } catch {
    // This is an opaque correlation token, never an authentication token.
    for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
  }
  bytes[6] = (bytes[6] & 15) | 64;
  bytes[8] = (bytes[8] & 63) | 128;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}:${sequence}`;
}

function commandReceiptSnapshot(error, command) {
  if (!command?.receiptKey) return;
  error.receiptKey = command.receiptKey;
  error.commandId = command.commandId;
  error.orderIdentities = Object.freeze((command.orderIdentities ?? []).map((receipt) => Object.freeze({ ...receipt })));
  error.actualClientOrderIds = Object.freeze([...new Set(error.orderIdentities
    .filter((receipt) => receipt.kind === 'clientOrderId').map((receipt) => receipt.identity))]);
}

function interruptedCommand(error, command, interrupted) {
  if (!command) return;
  error.command = command.op;
  if (interrupted && WRITE_COMMANDS.has(command.op)) {
    error.outcome = 'unknown';
    error.message += '; command outcome is unknown. Reconcile before retrying';
  }
  if (command.clientOrderId !== undefined) error.clientOrderId = command.clientOrderId;
  if (command.clientOrderIds?.length) error.clientOrderIds = command.clientOrderIds;
  commandReceiptSnapshot(error, command);
}

export class CommandTimeoutError extends Error {
  constructor(timeoutMs, command) {
    super(`Engine command timed out after ${timeoutMs} ms`);
    this.name = 'CommandTimeoutError';
    this.code = 'COMMAND_TIMEOUT';
    this.timeoutMs = timeoutMs;
    interruptedCommand(this, command, true);
  }
}

export class CommandDeadlineError extends Error {
  constructor(cause, command) {
    super('Unable to establish engine command deadline', { cause });
    this.name = 'CommandDeadlineError';
    this.code = 'COMMAND_DEADLINE_FAILED';
    interruptedCommand(this, command, true);
  }
}

export class CommandProtocolError extends Error {
  constructor(cause, command) {
    super('Invalid exchange engine command response', { cause });
    this.name = 'CommandProtocolError';
    this.code = 'ENGINE_PROTOCOL_ERROR';
    interruptedCommand(this, command, true);
  }
}

export class EngineDisposedError extends Error {
  constructor(command, interrupted = false) {
    super('engine disposed');
    this.name = 'EngineDisposedError';
    this.code = 'ENGINE_DISPOSED';
    interruptedCommand(this, command, interrupted);
  }
}

/** A trapped engine cannot be reused. An interrupted write may already exist at the venue. */
export class EngineDeadError extends Error {
  constructor(cause, command) {
    super('Exchange engine stopped unexpectedly', { cause });
    this.name = 'EngineDeadError';
    this.code = 'ENGINE_DEAD';
    if (command) {
      this.command = command.op;
      if (WRITE_COMMANDS.has(command.op)) {
        this.outcome = 'unknown';
        this.message = 'Exchange engine stopped; command outcome is unknown. Reconcile before retrying';
      }
      if (command.clientOrderId !== undefined) this.clientOrderId = command.clientOrderId;
      if (command.clientOrderIds?.length) this.clientOrderIds = command.clientOrderIds;
      commandReceiptSnapshot(this, command);
    }
  }
}

export class EngineHost {
  constructor(engine, transport, handlers = {}, options = {}) {
    this.commandTimeoutMs = commandTimeout(options.commandTimeoutMs === undefined ? DEFAULT_COMMAND_TIMEOUT_MS : options.commandTimeoutMs);
    this.engine = engine;
    this.transport = transport;
    this.handlers = handlers;
    this.sockets = new Map();
    this.timers = new Map();
    this.commands = new Map();
    this.requests = new Map();
    this.signings = new Map();
    this.nextCommand = 1;
    this.receiptSession = undefined;
    this.disposed = false;
    this.deadError = undefined;
    this.wsEncoder = undefined;
    this.wsScratch = undefined;
    this.wsScratchBusy = false;
  }

  start() {
    if (this.disposed) return;
    this.run(this.invoke(this.engine.start, this.transport.now()));
  }

  get dead() { return this.deadError !== undefined; }

  // Preserve native argument counts without per-message closures or rest arrays.
  invoke(fn, a, b, c, d) {
    if (this.disposed) return;
    try {
      switch (arguments.length) {
        case 1: return fn.call(this.engine);
        case 2: return fn.call(this.engine, a);
        case 3: return fn.call(this.engine, a, b);
        case 4: return fn.call(this.engine, a, b, c);
        default: return fn.call(this.engine, a, b, c, d);
      }
    } catch (error) {
      if (typeof WebAssembly === 'undefined' || !(error instanceof WebAssembly.RuntimeError)) throw error;
      this.deadError = new EngineDeadError(error);
      this.disposed = true;
      try { this.cleanup(); }
      finally { this.handlers.onDead?.(this.deadError); }
    }
  }

  /** Send a command; resolves with its result for commands that report one. */
  command(command, options) {
    if (this.disposed) return Promise.reject(this.deadError ?? new EngineDisposedError());
    let timeoutMs;
    try { timeoutMs = commandTimeout(options?.timeoutMs === undefined ? this.commandTimeoutMs : options.timeoutMs); }
    catch (error) { return Promise.reject(error); }
    const reply = command.op === 'publicFeedUpdate' || command.op === 'setTickerSymbols' || command.op === 'setTickInterval' || command.op === 'subscribeOrderBook' || command.op === 'unsubscribeOrderBook'
      || command.op === 'subscribeCandles' || command.op === 'unsubscribeCandles'
      ? undefined
      : this.nextCommand++;
    const clientOrderIds = reply !== undefined && Array.isArray(command.orders)
      ? command.orders.map((order) => order?.clientOrderId).filter((id) => typeof id === 'string') : undefined;
    const identity = reply === undefined ? undefined : { op: command.op,
      clientOrderId: command.clientOrderId ?? (clientOrderIds?.length === 1 ? clientOrderIds[0] : undefined), clientOrderIds,
      receiptKey: `${this.receiptSession ??= receiptSession()}:${reply}`, commandId: reply, orderIdentities: [] };
    const promise = reply === undefined
      ? Promise.resolve()
      : new Promise((resolve, reject) => this.commands.set(reply, { resolve, reject, identity }));
    try {
      const startedAt = this.transport.now();
      const json = JSON.stringify(reply === undefined ? command : { ...command, command: reply });
      this.run(this.invoke(this.engine.command, json, startedAt));
      if (reply === undefined && this.deadError) return Promise.reject(this.deadError);
      const pending = this.commands.get(reply);
      if (pending) {
        // Synchronous completions need no timer. Use the transport clock so replay
        // and virtual transports retain their normal deadline semantics.
        try {
          const clock = typeof this.transport.setTimeout === 'function' && typeof this.transport.clearTimeout === 'function'
            ? this.transport : globalThis;
          const timer = clock.setTimeout(() => {
            const current = this.commands.get(reply);
            if (current) this.settleCommand(reply, new CommandTimeoutError(timeoutMs, current.identity), true);
          }, Math.max(0, timeoutMs - (this.transport.now() - startedAt)));
          pending.clearDeadline = () => clock.clearTimeout(timer);
          // Also tolerate synchronous timer adapters and disposal during scheduling.
          if (this.commands.get(reply) !== pending) {
            try { pending.clearDeadline(); } catch { /* Settlement already won. */ }
          }
        } catch (cause) {
          this.settleCommand(reply, new CommandDeadlineError(cause, pending.identity), true);
        }
      }
    } catch (error) {
      if (reply === undefined) return Promise.reject(error);
      if (!this.commands.has(reply) && error instanceof UnknownEngineActionError) {
        // A known completion cannot be undone, but protocol failures remain visible.
        if (this.handlers.onLog) this.handlers.onLog('error', error.message);
        else console.error(error);
      }
      const pending = this.commands.get(reply);
      if (pending && error instanceof Error) {
        try { commandReceiptSnapshot(error, pending.identity); } catch { /* Preserve even a frozen validation error. */ }
      }
      this.settleCommand(reply, error instanceof UnknownEngineActionError
        ? new CommandProtocolError(error, pending?.identity) : error, true);
    }
    return promise;
  }

  settleCommand(id, value, rejected = false) {
    const pending = this.commands.get(id);
    if (!pending) return;
    this.commands.delete(id);
    try { pending.clearDeadline?.(); } catch { /* A timer adapter cannot strand the caller. */ }
    if (rejected) pending.reject(value);
    else pending.resolve(value);
  }

  commandProgress(id, receipts, complete, late) {
    if (!Number.isSafeInteger(id) || id <= 0 || id >= this.nextCommand || !this.receiptSession) return;
    const event = Object.freeze({ receiptKey: `${this.receiptSession}:${id}`, commandId: id,
      receipts: Object.freeze(receipts), complete, late });
    try { this.handlers.onCommandProgress?.(event); }
    catch (cause) { throw new CommandProgressCallbackError(cause); }
  }

  commandDiagnostic(level, message) {
    try { this.handlers.onLog?.(level, message); }
    catch (cause) { throw new CommandDiagnosticCallbackError(cause); }
  }

  /**
   * Hand everything that changed since the previous flush to `onFrame` as one
   * ArrayBuffer (read it with wire.mjs's Frame; transfer it, don't copy it).
   * Call once per flush interval; with `marketOnly`, only books, candles and
   * fills go out, for flushing those at the venue's faster book cadence.
   */
  flush(marketOnly = false) {
    if (this.disposed) return;
    const frame = this.invoke(this.engine.takeFrame, this.transport.now(), marketOnly);
    if (frame) this.handlers.onFrame?.(frame.buffer);
  }

  /** The whole current state as one frame (an ArrayBuffer), for a consumer
   * that starts late or skipped frames; later flushes apply on top of it. */
  snapshot() {
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    const frame = this.invoke(this.engine.snapshotFrame, this.transport.now());
    if (this.deadError) throw this.deadError;
    return frame.buffer;
  }

  /** Current private state and unflushed catalog; caller applies/forwards it. */
  accountSnapshot() {
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    if (typeof this.engine.accountSnapshotFrame !== 'function') throw new Error('Engine does not support private account snapshots');
    const frame = this.invoke(this.engine.accountSnapshotFrame, this.transport.now());
    if (this.deadError) throw this.deadError;
    return frame.buffer;
  }

  accountId() {
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    const id = this.engine.accountId ? this.invoke(this.engine.accountId) : undefined;
    if (this.deadError) throw this.deadError;
    return id;
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    this.cleanup();
  }

  cleanup() {
    const attempt = (fn) => {
      try { fn(); } catch { /* A broken transport must not strand the remaining resources. */ }
    };
    for (const [id, pending] of this.commands) this.settleCommand(id, this.deadError
      ? new EngineDeadError(this.deadError.cause, pending.identity) : new EngineDisposedError(pending.identity, true), true);
    for (const controller of this.requests.values()) attempt(() => controller.abort());
    this.requests.clear();
    for (const pending of this.signings.values()) {
      attempt(() => pending.controller.abort());
      attempt(() => this.transport.clearTimeout(pending.timer));
    }
    this.signings.clear();
    for (const socket of this.sockets.values()) attempt(() => socket.close());
    for (const handle of this.timers.values()) attempt(() => this.transport.clearTimeout(handle));
    this.sockets.clear();
    this.wsScratch = undefined;
    this.timers.clear();
    attempt(() => this.transport.dispose?.());
  }

  /** Feed a response to the engine in chunks of at most `CHUNK_BYTES`. */
  async receive(id, { status, headers = '', body }) {
    if (this.disposed) return;
    const { engine, transport } = this;
    this.run(this.invoke(engine.httpStart, id, status, headers, transport.now()));
    if (this.disposed) return;
    try {
      if (body instanceof Uint8Array) {
        for (let at = 0; at < body.length; at += CHUNK_BYTES) {
          if (this.disposed) return;
          this.run(this.invoke(engine.httpChunk, id, body.subarray(at, at + CHUNK_BYTES), transport.now()));
        }
      } else {
        for await (const chunk of body) {
          if (this.disposed) return;
          for (let at = 0; at < chunk.length; at += CHUNK_BYTES) {
            if (this.disposed) return;
            this.run(this.invoke(engine.httpChunk, id, chunk.subarray(at, at + CHUNK_BYTES), transport.now()));
          }
        }
      }
    } catch (error) {
      if (this.disposed) return;
      this.run(this.invoke(engine.httpFailed, id, String(error?.message ?? error), transport.now()));
      return;
    }
    if (this.disposed) return;
    this.run(this.invoke(engine.httpEnd, id, transport.now()));
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
      if (!this.disposed) this.run(this.invoke(this.engine.httpFailed, id, String(error?.message ?? error), this.transport.now()));
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
      if (error) {
        controller.abort();
        this.run(this.invoke(this.engine.payloadSignFailed, id, String(error?.message ?? error), this.transport.now()));
      } else {
        this.run(this.invoke(this.engine.payloadSigned, id, result.signature, result.headers, this.transport.now()));
      }
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
    let unknown;
    let callbackFailure;
    for (const action of JSON.parse(actionsJson)) {
      // Unknown protocol actions remain errors, but cannot discard the batch's
      // subsequent command completions. Ordinary callbacks still propagate.
      try { this.execute(action); }
      catch (error) {
        if (error instanceof CommandProgressCallbackError || error instanceof CommandDiagnosticCallbackError || error instanceof AccountAuthCallbackError) { callbackFailure ??= error; continue; }
        if (!(error instanceof UnknownEngineActionError)) throw error;
        unknown ??= error;
      }
    }
    if (callbackFailure) {
      // Broken diagnostic consumers must not turn a dispatched write into an
      // ordinary rejection that a caller could mistake for a safe retry.
      // Diagnostic listeners may have already displayed the command error
      // before throwing. Reporting their own failure must not add another toast.
      const level = callbackFailure instanceof CommandProgressCallbackError ? 'error' : 'warning';
      let reported = false;
      try {
        if (this.handlers.onLog) {
          this.handlers.onLog(level, callbackFailure.message);
          reported = true;
        }
      } catch { /* Fall back independently of the outcome of the native command. */ }
      if (!reported) {
        try {
          if (level === 'warning') console.warn(callbackFailure);
          else console.error(callbackFailure);
        } catch { /* Diagnostics are best effort. */ }
      }
    }
    if (unknown) throw unknown;
  }

  execute(action) {
    if (this.disposed) return;
    const { engine, transport } = this;
    switch (action?.op) {
      case 'signPayload':
        this.signPayload(action);
        break;
      case 'wsConnect': {
        const id = action.socket;
        const wsMessageBytes = typeof engine.wsMessageBytes === 'function' ? engine.wsMessageBytes : undefined;
        const wsMessage = typeof engine.wsMessageActions === 'function' ? engine.wsMessageActions : engine.wsMessage;
        if (action.headers?.length && !transport.webSocketHeaders) {
          this.handlers.onLog?.('warning', 'WebSocket upgrade headers unavailable; continuing REST reconciliation');
          transport.setTimeout(() => {
            if (!this.disposed) this.run(this.invoke(engine.wsClose, id, 1006, transport.now()));
          }, 0);
          break;
        }
        const socket = transport.connect(action.url, {
          binary: action.binary === true,
          channel: action.channel,
          context: action.context,
          headers: action.headers && Object.fromEntries(action.headers),
          onOpen: () => { if (!this.disposed) this.run(this.invoke(engine.wsOpen, id, transport.now())); },
          onMessage: (text) => {
            if (this.disposed) return;
            if (!wsMessageBytes) {
              this.run(this.invoke(wsMessage, id, text, transport.now()));
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
              actions = this.invoke(wsMessageBytes, id, bytes, transport.now());
            } finally {
              if (scratch) this.wsScratchBusy = false;
            }
            this.run(actions);
          },
          onBinary: (data) => {
            if (this.disposed || !this.sockets.has(id)) return;
            if (engine.wsBinary) this.run(this.invoke(engine.wsBinary, id, data, transport.now()));
            else {
              const active = this.sockets.get(id);
              this.sockets.delete(id);
              active.close();
              this.run(this.invoke(engine.wsClose, id, 1003, transport.now()));
            }
          },
          onClose: (code) => {
            if (this.disposed || !this.sockets.has(id)) return;
            this.sockets.delete(id);
            this.run(this.invoke(engine.wsClose, id, code ?? 1006, transport.now()));
          },
        });
        // A synchronous transport may call onOpen before returning its socket.
        if (this.disposed) {
          if (this.dead) { try { socket.close(); } catch { /* Fatal cleanup is best effort. */ } }
          else socket.close();
        } else this.sockets.set(id, socket);
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
          this.run(this.invoke(engine.timer, id, transport.now()));
        }, Math.max(0, action.at - transport.now()));
        if (this.disposed) transport.clearTimeout(handle);
        else this.timers.set(id, handle);
        break;
      }
      case 'clearTimer':
        transport.clearTimeout(this.timers.get(action.timer));
        this.timers.delete(action.timer);
        break;
      case 'deactivate':
        this.handlers.onDeactivate?.();
        break;
      case 'accountAuth': {
        if (!['primary', 'secondary'].includes(action.credential)
          || !['account', 'balance', 'orders', 'positions', 'settings'].includes(action.scope)
          || !['accepted', 'rejected'].includes(action.outcome)
          || Object.keys(action).some((key) => !['op', 'credential', 'scope', 'outcome'].includes(key))) {
          throw new UnknownEngineActionError('Invalid account authentication action');
        }
        try {
          this.handlers.onAccountAuth?.(Object.freeze({ credential: action.credential, scope: action.scope, outcome: action.outcome }));
        } catch (cause) { throw new AccountAuthCallbackError(cause); }
        break;
      }
      case 'log':
        this.handlers.onLog?.(action.level, action.message);
        break;
      case 'commandError': {
        // Only an explicitly attributed native error participates in command
        // reporting. Background warnings/errors retain their ordinary path.
        const pending = this.commands.get(action.command);
        if (pending && pending.firstCommandError === undefined) pending.firstCommandError = action.message;
        this.commandDiagnostic('warning', action.message);
        break;
      }
      case 'commandProgress': {
        const pending = this.commands.get(action.command);
        const receipts = [];
        for (const receipt of action.receipts) {
          if (typeof receipt.symbol !== 'string' || typeof receipt.identity !== 'string'
            || !['clientOrderId', 'clientOrderIndex', 'orderCode'].includes(receipt.kind)) continue;
          if (!receipts.some((row) => row.symbol === receipt.symbol && row.kind === receipt.kind && row.identity === receipt.identity)) {
            receipts.push(Object.freeze({ symbol: receipt.symbol, kind: receipt.kind, identity: receipt.identity }));
          }
        }
        if (pending) {
          for (const receipt of receipts) {
            if (!pending.identity.orderIdentities.some((row) => row.symbol === receipt.symbol && row.kind === receipt.kind && row.identity === receipt.identity)) {
              pending.identity.orderIdentities.push(receipt);
            }
          }
        }
        this.commandProgress(action.command, receipts, false, !pending);
        break;
      }
      case 'done': {
        const pending = this.commands.get(action.command);
        if (!pending) {
          // Late definitive results close the receipt stream without a tombstone.
          // Malformed/duplicate results still cannot change an earlier outcome.
          try { if (action.error === undefined && action.result !== undefined) JSON.parse(action.result); }
          catch { break; }
          this.commandProgress(action.command, [], true, true);
          break;
        }
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
          commandReceiptSnapshot(error, pending.identity);
          this.settleCommand(action.command, error, true);
        }
        else {
          let result;
          try { result = action.result === undefined ? undefined : JSON.parse(action.result); }
          catch (cause) { this.settleCommand(action.command, new CommandProtocolError(cause, pending.identity), true); break; }
          this.settleCommand(action.command, result);
        }
        // Settlement wins before any consumer runs: a broken toast/log handler
        // cannot turn known acceptance into a retryable failure or skip the tail.
        let callbackFailure;
        try { this.commandProgress(action.command, [], true, false); }
        catch (error) { callbackFailure = error; }
        if (action.error === undefined && pending.firstCommandError !== undefined) {
          try { this.commandDiagnostic('error', pending.firstCommandError); }
          catch (error) { callbackFailure ??= error; }
        }
        if (callbackFailure) throw callbackFailure;
        break;
      }
      default:
        throw new UnknownEngineActionError(`unknown engine action: ${action?.op}`);
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
export const socketMessages = (onMessage, onError, binary = false) => {
  let pending;
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
  return (data) => {
    if (typeof data === 'string' && !pending) return onMessage(data);
    const next = (pending ?? Promise.resolve()).then(() => decode(data)).then(onMessage);
    pending = next;
    next.catch(onError).finally(() => { if (pending === next) pending = undefined; });
  };
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
    const reportClose = (code) => { if (!closed) { closed = true; onClose(code); } };
    ws.addEventListener('error', () => reportClose(1006));
    ws.addEventListener('open', () => { if (!closed) onOpen(); });
    const receive = socketMessages((data) => {
      if (!closed) { if (typeof data === 'string') onMessage(data); else onBinary?.(data); }
    }, () => ws.close(), binary);
    ws.addEventListener('message', (event) => receive(event.data));
    ws.addEventListener('close', (event) => reportClose(event.code));
    return { send: (text) => ws.send(text), close: () => ws.close() };
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
