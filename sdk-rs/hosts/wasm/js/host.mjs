import { ManagedHttp, prepareFetch } from './managed-http.mjs';
export { HttpPreparationError } from './managed-http.mjs';

function signingNonce(value) {
  if (typeof value !== 'string' || value.length < 1 || value.length > 20
    || (value.length > 1 && value[0] === '0')) return false;
  for (let i = 0; i < value.length; i++) {
    const digit = value.charCodeAt(i);
    if (digit < 48 || digit > 57) return false;
  }
  return value.length < 20 || value <= '18446744073709551615';
}

// Return only closed clock/decimal-nonce facts, never arbitrary provider data.
function signingReceipt(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)
    || Object.keys(value).some((key) => key !== 'stampMs' && key !== 'expiryMs' && key !== 'nonce')) return undefined;
  const { stampMs, expiryMs, nonce } = value;
  if (nonce !== undefined && !signingNonce(nonce)) return undefined;
  if ([stampMs, expiryMs].some((time) => time !== undefined && (!Number.isSafeInteger(time) || time < 0))
    || (stampMs !== undefined && expiryMs !== undefined && expiryMs <= stampMs)) return undefined;
  return { ...(stampMs === undefined ? {} : { stampMs }), ...(expiryMs === undefined ? {} : { expiryMs }), ...(nonce === undefined ? {} : { nonce }) };
}

function signingMetaKeys(value, keys) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

function checkedSigningMeta(json, task) {
  if (typeof json !== 'string' || json.length > 1_048_576) return undefined;
  let meta;
  try { meta = JSON.parse(json); } catch { return undefined; }
  const id = (value) => Number.isSafeInteger(value) && value >= 0;
  if (!signingMetaKeys(meta, ['task', 'owner', 'deadlineMs', 'slot', 'role'])
    || !id(meta.task) || meta.task !== task || !Number.isFinite(meta.deadlineMs) || meta.deadlineMs < 0
    || (meta.slot !== 'primary' && meta.slot !== 'secondary')) return undefined;
  const owner = meta.owner;
  if (!owner || !id(owner.generation)) return undefined;
  if (owner.kind === 'managed') {
    if (!signingMetaKeys(owner, ['kind', 'request', 'generation']) || !id(owner.request)) return undefined;
  } else if (owner.kind !== 'session' || !signingMetaKeys(owner, ['kind', 'generation'])) return undefined;
  const role = meta.role;
  if (!role) return undefined;
  let retainedRole;
  if (role.kind === 'read' || role.kind === 'primaryRead') {
    if (!signingMetaKeys(role, role.kind === 'read' ? ['kind', 'scope'] : ['kind', 'reason', 'scope'])
      || (role.scope !== null && !['account', 'balance', 'orders', 'positions', 'settings'].includes(role.scope))) return undefined;
    if (role.kind === 'primaryRead'
      && !['clientOrderLookup', 'postUnknownOrderRead', 'degradedRead', 'catalog', 'history', 'keyPermissions'].includes(role.reason)) return undefined;
    retainedRole = { kind: role.kind, ...(role.kind === 'primaryRead' ? { reason: role.reason } : {}), scope: role.scope };
  } else {
    if (!['mutation', 'sessionAuth'].includes(role.kind) || !signingMetaKeys(role, ['kind'])) return undefined;
    retainedRole = { kind: role.kind };
  }
  if ((owner.kind === 'session') !== (role.kind === 'sessionAuth')
    || (meta.slot === 'secondary' && role.kind !== 'read')) return undefined;
  return Object.freeze({ task: meta.task,
    owner: Object.freeze({ kind: owner.kind, ...(owner.kind === 'managed' ? { request: owner.request } : {}), generation: owner.generation }),
    deadlineMs: meta.deadlineMs, slot: meta.slot, role: Object.freeze(retainedRole) });
}

function checkedSigningResult(expected, count, result) {
  const receipt = signingReceipt(result?.receipt);
  const rawValues = result?.values;
  if (!receipt || !expected || receipt.stampMs !== expected.stampMs || receipt.expiryMs !== expected.expiryMs || receipt.nonce !== expected.nonce
    || !Array.isArray(rawValues) || rawValues.length !== count) return undefined;
  const values = [];
  for (let i = 0; i < count; i++) {
    const rawBytes = rawValues[i];
    if (!Array.isArray(rawBytes)) return undefined;
    const size = rawBytes.length;
    if (!Number.isSafeInteger(size) || size > 1048576 || size < 0) return undefined;
    const bytes = [];
    for (let j = 0; j < size; j++) {
      const byte = rawBytes[j];
      if (!Number.isInteger(byte) || byte < 0 || byte > 255) return undefined;
      bytes.push(byte);
    }
    values.push(bytes);
  }
  let headers;
  const rawHeaders = result.headers;
  if (rawHeaders !== undefined) {
    if (!Array.isArray(rawHeaders)) return undefined;
    const count = rawHeaders.length;
    if (!Number.isSafeInteger(count) || count < 0 || count > 32) return undefined;
    let total = 0;
    const encoder = new TextEncoder();
    const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
    headers = [];
    for (let i = 0; i < count; i++) {
      const pair = rawHeaders[i];
      if (!Array.isArray(pair) || pair.length !== 2) return undefined;
      const name = pair[0];
      const value = pair[1];
      if (typeof name !== 'string' || !/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(name) || name.length > 256
        || typeof value !== 'string' || value.length > 8192 || /[\x00-\x08\x0a-\x1f\x7f]/.test(value)) return undefined;
      const bytes = encoder.encode(value);
      if (bytes.length > 8192 || decoder.decode(bytes) !== value) return undefined;
      total += name.length + bytes.length;
      if (total > 32768) return undefined;
      headers.push([name, value]);
    }
  }
  return { values, receipt,
    ...(headers?.length ? { headers } : {}) };
}

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

const socketStats = () => ({
  open: 0, initialized: 0, lastReceived: 0, lastCloseCode: undefined, lastCloseAt: 0, closes: 0, closeStreak: 0,
});

const CHUNK_BYTES = 16 * 1024;
const MAX_DECOMPRESSED_BYTES = 16 * 1024 * 1024;
export const DEFAULT_COMMAND_TIMEOUT_MS = 120_000;
const commandTimeout = (value) => {
  if (!Number.isSafeInteger(value) || value <= 0 || value > 2_147_483_647) {
    throw new RangeError('Command timeout must be a positive safe integer <= 2147483647');
  }
  return value;
};
const WRITE_COMMANDS = new Set(['placeOrders', 'cancelOrders', 'cancelOrdersWithSymbols', 'amendOrder',
  'amendOrderWithSymbol', 'cancelSymbolOrders', 'setLeverage', 'setMarginType', 'setPositionMode', 'setStpMode',
  'setAccountTier', 'scheduleCancel']);
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

const entropyFailure = (message) => new ExchangeError(message, { kind: 'invalidInput' });

function checkedEntropy(source, name) {
  if (typeof source !== 'function') throw entropyFailure(`${name} entropy provider must be a function`);
  return (length) => {
    try {
      if (!Number.isSafeInteger(length) || length < 0 || length > 0xffff_ffff)
        throw new Error('Invalid entropy length');
      const bytes = source(length);
      if (!(bytes instanceof Uint8Array) || bytes.length !== length)
        throw new Error('Invalid entropy bytes');
      // A provider may retain and reuse its storage; neither host nor engine owns it.
      return new Uint8Array(bytes);
    } catch {
      throw entropyFailure(`${name} entropy provider failed`);
    }
  };
}

function productionEntropy() {
  try {
    const crypto = globalThis.crypto;
    const fill = crypto?.getRandomValues;
    if (typeof fill !== 'function') throw new Error('Web Crypto unavailable');
    return checkedEntropy((length) => {
      const bytes = new Uint8Array(length);
      // Web Crypto limits each call to 65536 bytes.
      for (let offset = 0; offset < length; offset += 65536)
        fill.call(crypto, bytes.subarray(offset, Math.min(offset + 65536, length)));
      return bytes;
    }, 'Identity');
  } catch {
    throw entropyFailure('Cryptographic host identity entropy is unavailable');
  }
}

function receiptSession(fill) {
  const bytes = fill(16);
  const sequence = ++nextReceiptSession;
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

const ERROR_KINDS = new Set(['rejected', 'invalidInput', 'insufficientBalance', 'authInvalid', 'authExpired',
  'authPermission', 'rateLimited', 'network', 'timeout', 'venueUnavailable', 'unsupported', 'internal']);
const ERROR_OUTCOMES = new Set(['notSent', 'rejected', 'unknown']);
const text = (value) => (typeof value === 'string' && value ? value : undefined);
const count = (value) => (Number.isSafeInteger(value) && value >= 0 ? value : undefined);

/** A closed, cloneable approval record. Never infer approval from an error message. */
export const builderAuthorizationOf = (value) => {
  if (!value || typeof value !== 'object' || value.code !== 'GRVT_BUILDER_UNAUTHORIZED') return undefined;
  const uint32 = (n) => Number.isInteger(n) && n >= 0 && n <= 0xffff_ffff;
  if (!uint32(value.requiredRaw) || value.requiredRaw === 0) return undefined;
  if (value.reason === 'unauthorized' && value.authorizedRaw === undefined) {
    return { code: value.code, reason: value.reason, requiredRaw: value.requiredRaw };
  }
  if (value.reason === 'cap-too-low' && uint32(value.authorizedRaw) && value.authorizedRaw < value.requiredRaw) {
    return { code: value.code, reason: value.reason, requiredRaw: value.requiredRaw, authorizedRaw: value.authorizedRaw };
  }
  return undefined;
};

/** A classified venue failure (`engine/src/error.rs`). `message` is the text
 * hosts have always shown; `name` stays `Error` so displayed text is unchanged. */
export class ExchangeError extends Error {
  constructor(message, detail = {}) {
    super(message);
    const builder = builderAuthorizationOf(detail.builderAuthorization);
    if (builder) Object.assign(this, builder);
    if (ERROR_KINDS.has(detail.kind)) this.kind = detail.kind;
    if (ERROR_OUTCOMES.has(detail.outcome)) this.outcome = detail.outcome;
    const venueCode = text(detail.venueCode);
    if (venueCode) this.venueCode = venueCode;
    const venueMessage = text(detail.venueMessage);
    if (venueMessage) this.venueMessage = venueMessage;
    if (Number.isFinite(detail.retryAfterMs) && detail.retryAfterMs >= 0) this.retryAfterMs = detail.retryAfterMs;
    const ids = detail.partial?.orderIds;
    if (Array.isArray(ids) && ids.length && ids.every((id) => typeof id === 'string')) {
      this.partialOrderIds = [...ids];
      this.partialOrderCount = ids.length;
      const total = count(detail.partial.total);
      if (total !== undefined) this.totalOrderCount = total;
      const failedIndex = count(detail.partial.failedIndex);
      if (failedIndex !== undefined) this.failedOrderIndex = failedIndex;
    }
  }
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
    // A trap (a Rust panic) is the engine's own fault, never a venue answer.
    this.kind = 'internal';
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
    const identity = options.entropy === undefined ? productionEntropy() : checkedEntropy(options.entropy, 'Identity');
    const signing = options.signingEntropy === undefined ? undefined : checkedEntropy(options.signingEntropy, 'Signing');
    for (const [source, setter, name] of [
      [options.entropy === undefined ? undefined : identity, 'setEntropy', 'identity'],
      [signing, 'setSigningEntropy', 'signing'],
    ]) {
      if (source !== undefined && typeof engine[setter] !== 'function')
        throw entropyFailure(`Engine does not support ${name} entropy injection`);
    }
    for (const [source, setter] of [
      [options.entropy === undefined ? undefined : identity, 'setEntropy'],
      [signing, 'setSigningEntropy'],
    ]) {
      if (source === undefined) continue;
      try { engine[setter](source); }
      catch { throw entropyFailure('Engine entropy must be configured before startup'); }
    }
    if (engine.requiresExternalSigner?.() && options.signer === undefined)
      throw new ExchangeError('Engine signing provider is required', { message:'Engine signing provider is required', kind:'invalidInput', outcome:'notSent' });
    if (options.signer !== undefined) {
      if (typeof options.signer !== 'function' || typeof engine.useExternalSigner !== 'function')
        throw new TypeError('Engine requires a callable signing provider');
      if (options.signerAlgorithms !== undefined && !Array.isArray(options.signerAlgorithms))
        throw new TypeError('Engine signer algorithms must be an array');
      engine.useExternalSigner(options.signerAlgorithms === undefined ? undefined : JSON.stringify(options.signerAlgorithms));
    }
    this.signer = options.signer;
    this.primitiveSignings = new Map();
    // Provider calls still running after their wait ended, for retirement joins.
    this.signingJoins = new Map();
    this.hostedSignings = new Map();
    this.entropy = identity;
    this.engine = engine;
    this.ownsEngine = options.ownsEngine === true;
    this.transport = transport;
    this.handlers = handlers;
    this.sockets = new Map();
    this.socketRecords = new Map();
    this.socketStats = { public: socketStats(), business: socketStats(), private: socketStats() };
    this.timers = new Map();
    this.commands = new Map();
    // Timed commands owning a sign attempt, HTTP request or managed preparation.
    this.stageCauses = new Map();
    this.stageCause = undefined;
    this.requests = new Map();
    this.managedHttp = new ManagedHttp(this);
    this.socketFailures = new Map();
    this.nextCommand = 1;
    this.receiptSession = undefined;
    this.disposed = false;
    this.started = false;
    this.deadError = undefined;
    this.wsEncoder = undefined;
    this.wsScratch = undefined;
    this.wsScratchBusy = false;
    this.algosStarted = false;
    this.algoTimer = undefined;
    this.algoTimerAt = undefined;
    this.algosStopped = undefined;
    this.algoStopAllWaiters = [];
  }

  start() {
    this.assertHttpHandoffIdle();
    if (this.disposed || this.started) return;
    this.started = true;
    this.run(this.invoke(this.engine.start, this.transport.now()));
  }

  get dead() { return this.deadError !== undefined; }

  deferHttpNotification(callback, value, binary = false) {
    const group = this.managedHttp?.groupHandoff;
    if (!group) return false;
    if (binary || group.notificationCount === group.notifications.length) {
      group.notificationFatal = true;
      return true;
    }
    const slot = group.notifications[group.notificationCount++];
    slot.callback = callback;
    slot.value = value;
    return true;
  }

  assertHttpHandoffIdle() {
    if (this.managedHttp?.groupHandoff) throw new Error('HTTP group handoff cannot be interrupted');
  }

  failEngine(error) {
    this.assertHttpHandoffIdle();
    if (this.disposed) return;
    this.deadError = new EngineDeadError(error);
    this.disposed = true;
    try { this.cleanup(); }
    finally { this.handlers.onDead?.(this.deadError); }
  }

  // Preserve native argument counts without per-message closures or rest arrays.
  invoke(fn, a, b, c, d) {
    this.assertHttpHandoffIdle();
    if (this.disposed) return;
    try {
      if (this.engine.failed?.()) {
        this.failEngine(new Error('Engine resource identities exhausted'));
        return;
      }
      let result;
      switch (arguments.length) {
        case 1: result = fn.call(this.engine); break;
        case 2: result = fn.call(this.engine, a); break;
        case 3: result = fn.call(this.engine, a, b); break;
        case 4: result = fn.call(this.engine, a, b, c); break;
        default: result = fn.call(this.engine, a, b, c, d); break;
      }
      if (this.engine.failed?.()) {
        this.failEngine(new Error('Engine resource identities exhausted'));
        return;
      }
      return result;
    } catch (error) {
      // Do not inspect Rust state after a trap: its mutable borrow may be held.
      const fatal = (typeof WebAssembly !== 'undefined' && error instanceof WebAssembly.RuntimeError)
        || error instanceof RangeError;
      if (!fatal) throw error;
      this.failEngine(error);
    }
  }

  /** Send a command; resolves with its result for commands that report one. */
  command(command, options) {
    this.assertHttpHandoffIdle();
    if (this.disposed) return Promise.reject(this.deadError ?? new EngineDisposedError());
    let timeoutMs;
    let onEvent;
    let onStage;
    try {
      if (command.op === 'scheduleCancel' && command.time != null
          && (!Number.isSafeInteger(command.time) || command.time < 0)) {
        throw new ExchangeError('scheduleCancel time must be a nonnegative safe integer UTC timestamp in milliseconds',
          { kind: 'invalidInput', outcome: 'notSent' });
      }
      timeoutMs = commandTimeout(options?.timeoutMs === undefined ? this.commandTimeoutMs : options.timeoutMs);
      onEvent = options?.onEvent;
      if (onEvent !== undefined && typeof onEvent !== 'function') throw new TypeError('Command event observer must be a function');
      onStage = options?.onStage;
      if (onStage !== undefined && typeof onStage !== 'function') throw new TypeError('Command stage observer must be a function');
    } catch (error) { return Promise.reject(error); }
    const reply = command.op === 'publicFeedUpdate' || command.op === 'setTickerSymbols' || command.op === 'setPollScale' || command.op === 'subscribeOrderBook' || command.op === 'unsubscribeOrderBook'
      || command.op === 'subscribeCandles' || command.op === 'unsubscribeCandles'
      ? undefined
      : this.nextCommand++;
    const clientOrderIds = reply !== undefined && Array.isArray(command.orders)
      ? command.orders.map((order) => order?.clientOrderId).filter((id) => typeof id === 'string') : undefined;
    let identity;
    try {
      identity = reply === undefined ? undefined : { op: command.op,
        clientOrderId: command.clientOrderId ?? (clientOrderIds?.length === 1 ? clientOrderIds[0] : undefined), clientOrderIds,
        receiptKey: `${this.receiptSession ??= receiptSession(this.entropy)}:${reply}`, commandId: reply, orderIdentities: [] };
    } catch (error) { return Promise.reject(error); }
    const promise = reply === undefined
      ? Promise.resolve()
      : new Promise((resolve, reject) => this.commands.set(reply, { resolve, reject, identity, onEvent, onStage }));
    try {
      const startedAt = this.transport.now();
      // Reply commands carry the caller's effective wait; the engine bounds
      // work that should not outlive it (RUST_COMMAND_TIMEOUT.md).
      const json = JSON.stringify(reply === undefined ? command : { ...command, command: reply, timeoutMs });
      const actions = this.invoke(this.engine.command, json, startedAt);
      this.stage(reply, 'accepted');
      this.runFor(reply, actions);
      if (reply === undefined && this.deadError) return Promise.reject(this.deadError);
      const pending = this.commands.get(reply);
      if (pending) {
        // Synchronous completions need no timer. Use the transport clock so replay
        // and virtual transports retain their normal deadline semantics.
        try {
          const clock = typeof this.transport.setTimeout === 'function' && typeof this.transport.clearTimeout === 'function'
            ? this.transport : globalThis;
          const notify = () => {
            if (this.deferHttpNotification(notify)) return;
            const current = this.commands.get(reply);
            if (current) this.settleCommand(reply, new CommandTimeoutError(timeoutMs, current.identity), true);
          };
          const timer = clock.setTimeout(notify, Math.max(0, timeoutMs - (this.transport.now() - startedAt)));
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

  /** Stamps a timed command's stage; observers cannot change the command. */
  stage(command, name) {
    const onStage = command === undefined ? undefined : this.commands.get(command)?.onStage;
    if (!onStage) return;
    try { onStage(name); } catch { /* Timing is diagnostic only. */ }
  }

  /** Records that the running timed command owns `key` (a sign, request or preparation). */
  ownStage(key) {
    const cause = this.stageCause;
    if (cause !== undefined && this.commands.get(cause)?.onStage) this.stageCauses.set(key, cause);
    return cause;
  }

  /** Runs actions on behalf of a timed command, so what they start is attributed to it. */
  runFor(cause, actionsJson) {
    const outer = this.stageCause;
    this.stageCause = cause;
    try { this.run(actionsJson); } finally { this.stageCause = outer; }
  }

  settleCommand(id, value, rejected = false) {
    this.assertHttpHandoffIdle();
    const pending = this.commands.get(id);
    if (!pending) return;
    this.commands.delete(id);
    for (const [key, cause] of this.stageCauses) if (cause === id) this.stageCauses.delete(key);
    try { pending.clearDeadline?.(); } catch { /* A timer adapter cannot strand the caller. */ }
    if (rejected) pending.reject(value);
    else pending.resolve(value);
  }

  commandProgress(id, receipts, complete, late) {
    this.assertHttpHandoffIdle();
    if (!Number.isSafeInteger(id) || id <= 0 || id >= this.nextCommand || !this.receiptSession) return;
    const event = Object.freeze({ receiptKey: `${this.receiptSession}:${id}`, commandId: id,
      receipts: Object.freeze(receipts), complete, late });
    try { this.handlers.onCommandProgress?.(event); }
    catch (cause) { throw new CommandProgressCallbackError(cause); }
  }

  commandDiagnostic(level, message) {
    this.assertHttpHandoffIdle();
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
    this.assertHttpHandoffIdle();
    if (this.disposed) return;
    const frame = this.invoke(this.engine.takeFrame, this.transport.now(), marketOnly);
    // A taken frame carries fills and state to the algos, which may wake them.
    this.armAlgos();
    if (frame) this.handlers.onFrame?.(frame.buffer);
  }

  /** Apply one data-source frame (its exact SCXF bytes) to a `frameFed` engine.
   * A snapshot resets the account state first. Runs whatever the ingest releases,
   * such as writes held for a placement ack; an ingest refusal throws. */
  applyFrame(frame, snapshot) {
    this.assertHttpHandoffIdle();
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    if (typeof this.engine.applyFrame !== 'function') throw new TypeError('This engine has no frame ingest');
    const bytes = frame instanceof Uint8Array ? frame : new Uint8Array(frame);
    this.run(this.invoke(this.engine.applyFrame, bytes, snapshot === true, this.transport.now()));
    if (this.deadError) throw this.deadError;
  }

  /** A frame-fed engine's data-source clock minus this host's, in ms, so its
   * fences compare frame times on one clock; until set they wait 30 s. */
  setSourceClockOffset(offsetMs) {
    this.assertHttpHandoffIdle();
    if (this.disposed || typeof this.engine.setSourceClockOffset !== 'function') return;
    this.invoke(this.engine.setSourceClockOffset, Number.isFinite(offsetMs) ? offsetMs : undefined);
  }

  /** The whole current state as one frame (an ArrayBuffer), for a consumer
   * that starts late or skipped frames; later flushes apply on top of it. */
  snapshot() {
    this.assertHttpHandoffIdle();
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    const frame = this.invoke(this.engine.snapshotFrame, this.transport.now());
    if (this.deadError) throw this.deadError;
    return frame.buffer;
  }

  /** Current private state and unflushed catalog; caller applies/forwards it. */
  accountSnapshot() {
    this.assertHttpHandoffIdle();
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    if (typeof this.engine.accountSnapshotFrame !== 'function') throw new Error('Engine does not support private account snapshots');
    const frame = this.invoke(this.engine.accountSnapshotFrame, this.transport.now());
    if (this.deadError) throw this.deadError;
    return frame.buffer;
  }

  /** The venue's capability record; fixed for the engine's lifetime, so read once. */
  capabilities() {
    this.assertHttpHandoffIdle();
    if (this.capabilityRecord) return this.capabilityRecord;
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    const json = this.invoke(this.engine.capabilities);
    if (this.deadError) throw this.deadError;
    return (this.capabilityRecord = JSON.parse(json));
  }

  /** Engine health counters; see `Health` in sdk-rs/engine/src/log.rs. */
  health() {
    this.assertHttpHandoffIdle();
    if (this.disposed) throw new Error('engine disposed');
    const json = this.engine.health?.();
    return json ? JSON.parse(json) : undefined;
  }

  /** Per-channel socket state since construction (`EngineSocketStats`). */
  socketHealth() {
    const copy = (stats) => ({ ...stats });
    return { public: copy(this.socketStats.public), business: copy(this.socketStats.business), private: copy(this.socketStats.private) };
  }

  /**
   * Close the open sockets of `channel` (or all) as an abnormal close, so the
   * venue runs its own reconnect. Returns how many sockets were closed.
   */
  reconnectSockets(channel) {
    this.assertHttpHandoffIdle();
    if (this.disposed) return 0;
    let closed = 0;
    for (const [id, record] of [...this.socketRecords]) {
      // A connect in flight is left alone, as in TS: a second connect would leak it.
      if ((channel !== undefined && record.channel !== channel) || !record.open || !this.sockets.has(id)) continue;
      const socket = this.sockets.get(id);
      this.sockets.delete(id);
      this.socketClosed(id, 1006);
      try { socket.close(); } catch { /* The engine still learns of the close below. */ }
      this.run(this.invoke(this.engine.wsClose, id, 1006, this.transport.now()));
      closed++;
      if (this.disposed) break;
    }
    return closed;
  }

  socketClosed(id, code) {
    this.assertHttpHandoffIdle();
    const record = this.socketRecords.get(id);
    if (!record) return;
    this.socketRecords.delete(id);
    const { stats } = record;
    if (record.open) stats.open--;
    if (record.initialized) stats.initialized--;
    // Frames still in flight from a closed socket must not count as recovery.
    record.open = false;
    record.initialized = false;
    stats.lastCloseCode = code;
    stats.lastCloseAt = Date.now();
    stats.closes++;
    stats.closeStreak++;
    this.socketChanged();
  }

  socketChanged() {
    this.assertHttpHandoffIdle();
    try { this.handlers.onSocketChange?.(); } catch { /* Health observers must not break socket handling. */ }
  }

  /** Start this account's Rust algos (chaser, TWAP, exec) on an `algos` engine
   * build. Their replies reach `handlers.onAlgoEmit(op, data)`. */
  algoStart(accountName) {
    this.assertHttpHandoffIdle();
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    if (typeof this.engine.algoStart !== 'function')
      throw new ExchangeError('This engine has no algos', { kind: 'unsupported', outcome: 'notSent' });
    this.invoke(this.engine.algoStart, accountName, this.transport.now(), Date.now());
    if (this.deadError) throw this.deadError;
    this.algosStarted = true;
    this.armAlgos();
  }

  get algosRunning() { return this.algosStarted && !this.disposed; }

  /** One algo op (`chaser:start`, `twap:stop`, `exec`, ...); throws if the engine refuses it. */
  algoOp(op, data) {
    this.assertHttpHandoffIdle();
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    if (!this.algosStarted) throw new ExchangeError('Algos are not running', { kind: 'unsupported', outcome: 'notSent' });
    this.runAlgos(this.invoke(this.engine.algoOp, op, JSON.stringify(data ?? null), this.transport.now(), Date.now()));
    if (this.deadError) throw this.deadError;
  }

  /** Stop every algo: TWAPs at once, chases once their cancels settle (bounded by
   * the engine). Resolves when they are gone; dispose the host after that. */
  algoDispose(reason) {
    this.assertHttpHandoffIdle();
    if (!this.algosRunning) return Promise.resolve();
    const stopped = this.algosStopped ??= {};
    stopped.promise ??= new Promise((resolve) => { stopped.resolve = resolve; });
    this.runAlgos(this.invoke(this.engine.algoDispose, this.transport.now(), Date.now(), reason));
    return stopped.promise;
  }

  /** A nuke's first step: stop every TWAP and chase, the algos staying up for
   * new ops. Resolves `{ stoppedChasers, stoppedTwaps, settled }` once chase
   * cancels settle (at most 10 s on the algo clock); `settled` is false when they
   * did not, or the algos ended first. */
  algoStopAll(reason) {
    this.assertHttpHandoffIdle();
    if (!this.algosRunning) return Promise.resolve({ stoppedChasers: [], stoppedTwaps: [], settled: true });
    if (typeof this.engine.algoStopAll !== 'function')
      throw new ExchangeError('This engine cannot stop its algos for a nuke', { kind: 'unsupported', outcome: 'notSent' });
    const stopped = new Promise((resolve) => this.algoStopAllWaiters.push(resolve));
    this.runAlgos(this.invoke(this.engine.algoStopAll, this.transport.now(), Date.now(), reason));
    if (this.deadError) throw this.deadError;
    return stopped;
  }

  /** Waits whose report can no longer come: the algos ended or the host went. */
  abandonAlgoStopAll() {
    for (const resolve of this.algoStopAllWaiters.splice(0)) resolve({ stoppedChasers: [], stoppedTwaps: [], settled: false });
  }

  advanceAlgos() {
    this.runAlgos(this.invoke(this.engine.algoAdvance, this.transport.now(), Date.now()));
  }

  runAlgos(actionsJson) {
    try { this.runActions(actionsJson); }
    finally { this.deliverAlgoEmits(); }
    this.armAlgos();
  }

  deliverAlgoEmits() {
    if (this.disposed) return;
    const json = this.invoke(this.engine.algoEmits);
    if (!json) return;
    for (const { op, data } of JSON.parse(json)) {
      // The answer to `algoStopAll`, oldest wait first; the worker never sees it.
      if (op === 'algos:stopped') {
        this.algoStopAllWaiters.shift()?.(data);
        continue;
      }
      try { this.handlers.onAlgoEmit?.(op, data); }
      catch (error) {
        try { this.handlers.onLog?.('error', `Algo reply handler failed: ${error?.message ?? error}`); }
        catch { /* An observer cannot strand the remaining replies. */ }
      }
    }
  }

  /** Keep exactly one timer armed at the engine's next algo deadline. Every engine
   * call that can wake algo work (completions, frames) ends here through `run`. */
  armAlgos() {
    if (!this.algosStarted || this.disposed) return;
    if (!this.invoke(this.engine.algoRunning)) {
      this.algosStarted = false;
      this.clearAlgoTimer();
      this.abandonAlgoStopAll();
      this.algosStopped?.resolve?.();
      return;
    }
    const at = this.invoke(this.engine.algoDeadline);
    if (this.disposed || at === this.algoTimerAt) return;
    this.clearAlgoTimer();
    if (at === undefined) return;
    let fired = false;
    let handle;
    const notify = () => {
      if (this.deferHttpNotification(notify)) return;
      if (fired || this.disposed || this.algoTimer !== handle) return;
      fired = true;
      this.algoTimer = this.algoTimerAt = undefined;
      this.advanceAlgos();
    };
    handle = this.transport.setTimeout(notify, Math.max(0, at - this.transport.now()));
    if (fired || this.disposed) return;
    this.algoTimer = handle;
    this.algoTimerAt = at;
  }

  clearAlgoTimer() {
    const handle = this.algoTimer;
    this.algoTimer = this.algoTimerAt = undefined;
    if (handle !== undefined) {
      try { this.transport.clearTimeout(handle); } catch { /* Its callback is revoked even if cancellation fails. */ }
    }
  }

  dispose() {
    this.assertHttpHandoffIdle();
    if (this.disposed) return;
    this.disposed = true;
    this.cleanup();
  }

  cleanup() {
    this.assertHttpHandoffIdle();
    const attempt = (fn) => {
      try { fn(); } catch { /* A broken transport must not strand the remaining resources. */ }
    };
    for (const [id, pending] of this.commands) this.settleCommand(id, this.deadError
      ? new EngineDeadError(this.deadError.cause, pending.identity) : new EngineDisposedError(pending.identity, true), true);
    for (const id of this.socketFailures.keys()) attempt(() => this.clearSocketFailure(id));
    for (const controller of this.requests.values()) attempt(() => controller.abort());
    this.requests.clear();
    attempt(() => this.managedHttp.dispose());
    for (const pending of this.primitiveSignings.values()) attempt(() => pending.cancel());
    this.primitiveSignings.clear();
    const addressSetupWaiter = this.addressSetupWaiter;
    this.addressSetupWaiter = undefined;
    attempt(() => addressSetupWaiter?.());
    for (const pending of this.hostedSignings.values()) attempt(() => pending.cancel());
    this.hostedSignings.clear();
    for (const socket of this.sockets.values()) attempt(() => socket.close());
    for (const handle of this.timers.values()) attempt(() => this.transport.clearTimeout(handle));
    this.sockets.clear();
    this.socketRecords.clear();
    this.wsScratch = undefined;
    this.timers.clear();
    attempt(() => this.clearAlgoTimer());
    this.algosStarted = false;
    attempt(() => this.abandonAlgoStopAll());
    attempt(() => this.algosStopped?.resolve?.());
    attempt(() => this.transport.dispose?.());
    // Release provider-owned keys on normal disposal instead of retaining the
    // wasm allocation (and an injected callback) until garbage collection.
    this.signer = undefined;
    const ownsEngine = this.ownsEngine;
    this.ownsEngine = false;
    if (ownsEngine) attempt(() => this.engine.free?.());
  }

  /** Feed a response to the engine in chunks of at most `CHUNK_BYTES`. */
  async receive(id, { status, headers = '', body }, active = () => true) {
    this.assertHttpHandoffIdle();
    if (this.disposed || !active()) return;
    const { engine, transport } = this;
    const cause = this.stageCauses.get(`http:${id}`);
    this.stageCauses.delete(`http:${id}`);
    this.stage(cause, 'response');
    const run = (actions) => this.runFor(cause, actions);
    run(this.invoke(engine.httpStart, id, status, headers, transport.now()));
    if (this.disposed || !active()) return;
    try {
      if (body instanceof Uint8Array) {
        for (let at = 0; at < body.length; at += CHUNK_BYTES) {
          if (this.disposed || !active()) return;
          run(this.invoke(engine.httpChunk, id, body.subarray(at, at + CHUNK_BYTES), transport.now()));
          if (this.disposed || !active()) return;
        }
      } else {
        for await (const chunk of body) {
          if (this.disposed || !active()) return;
          for (let at = 0; at < chunk.length; at += CHUNK_BYTES) {
            if (this.disposed || !active()) return;
            run(this.invoke(engine.httpChunk, id, chunk.subarray(at, at + CHUNK_BYTES), transport.now()));
            if (this.disposed || !active()) return;
          }
        }
      }
    } catch {
      if (this.disposed || !active()) return;
      run(this.invoke(engine.httpFailed, id, 'HTTP response body failed', transport.now()));
      return;
    }
    if (this.disposed || !active()) return;
    run(this.invoke(engine.httpEnd, id, transport.now()));
  }

  async request(action) {
    this.assertHttpHandoffIdle();
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
    } catch {
      if (!this.disposed) this.run(this.invoke(this.engine.httpFailed, id, 'HTTP transport request failed', this.transport.now()));
    } finally {
      this.requests.delete(id);
      this.stageCauses.delete(`http:${id}`);
    }
  }

  // Provider calls from dispatch and signing-only capture share cancellation,
  // bounds, receipt validation and sanitized closed errors.
  primitiveSigning(work, timeoutMs, execute, complete) {
    this.assertHttpHandoffIdle();
    const expectedReceipt = signingReceipt(work?.receipt);
    const expectedCount = work?.operations?.length;
    const controller = new AbortController();
    let settled = false;
    // Settles once the provider call itself has, not when waiting was abandoned.
    let workJoined;
    const joined = new Promise((resolve) => { workJoined = resolve; });
    let executing = false;
    const clearTimer = () => {
      if (pending.timer !== undefined) {
        try { this.transport.clearTimeout(pending.timer); } catch { /* Completion must still settle. */ }
      }
    };
    const finish = (result, code) => {
      if (settled) return;
      settled = true;
      clearTimer();
      if (code) {
        try { controller.abort(); } catch { /* Provider listeners cannot strand completion. */ }
      }
      complete(result, code);
      if (!executing) workJoined();
    };
    // Register correlation before arming: injected timers may fire inline or
    // throw. Both paths must retire the native capture and settle the caller.
    const pending = {
      controller,
      timer: undefined,
      joined,
      cancel: () => finish(undefined, 'unavailable'),
      start: () => {
        const notify = () => {
          if (this.deferHttpNotification(notify)) return;
          finish(undefined, 'timedOut');
        };
        try { pending.timer = this.transport.setTimeout(notify, timeoutMs); }
        catch { finish(undefined, 'unavailable'); return; }
        if (settled) { clearTimer(); return; }
        Promise.resolve().then(() => {
          if (controller.signal.aborted || this.disposed) return;
          if (!execute) { finish(undefined, 'unavailable'); return; }
          executing = true;
          return execute({ signal: controller.signal });
        }).then((result) => {
          if (controller.signal.aborted || this.disposed) return;
          const checked = checkedSigningResult(expectedReceipt, expectedCount, result);
          if (!checked) { finish(undefined, 'invalidResult'); return; }
          finish(checked);
        }).catch(() => finish(undefined, 'refused')).finally(() => workJoined());
      },
    };
    return pending;
  }

  /** Pre-start PublicAddress setup through an injected provider. The engine asks
   * to retire every task, success included; acknowledge only after that task's
   * provider call has actually settled, never on a timeout or abort alone. */
  prepareAddressSetup() {
    this.assertHttpHandoffIdle();
    const { engine } = this;
    if (this.disposed || this.started || !this.signer || typeof engine.setAddressSetupSupport !== 'function')
      return Promise.resolve();
    this.invoke(engine.setAddressSetupSupport, 'publicAddressOwned');
    this.addressSetupNegotiated = true;
    const ready = new Promise((resolve) => { this.addressSetupWaiter = resolve; });
    this.run(this.invoke(engine.initializeAddressSetup, this.transport.now(), false));
    this.checkAddressSetup();
    return ready;
  }

  checkAddressSetup() {
    const waiter = this.addressSetupWaiter;
    if (!waiter || (!this.disposed && !this.invoke(this.engine.addressSetupReady))) return;
    this.addressSetupWaiter = undefined;
    waiter();
  }

  retireAddressSetup({ task, token }) {
    this.primitiveSignings.get(task)?.cancel();
    Promise.resolve(this.signingJoins.get(task)).then(() => {
      if (this.disposed) return;
      this.run(this.invoke(this.engine.addressSetupRetired, token, this.transport.now()));
      this.checkAddressSetup();
    });
  }

  sign(action) {
    this.assertHttpHandoffIdle();
    if (this.engine.signingPending && !this.invoke(this.engine.signingPending, action.attempt)) return;
    const id = action.attempt;
    let meta;
    try {
      const readMeta = this.engine.signMeta;
      if (typeof readMeta === 'function') meta = checkedSigningMeta(this.invoke(readMeta, id), id);
    } catch (error) {
      if (this.disposed) throw error;
      // An ordinary export failure leaves the legacy context intact.
    }
    if (this.disposed) return;
    const cause = this.ownStage(`sign:${id}`);
    const pending = this.primitiveSigning(action.work, action.timeoutMs,
      this.signer && ((context) => this.signer(action.work, meta ? { ...context, meta } : context)), (result, code) => {
        if (this.primitiveSignings.get(id) !== pending || this.disposed) return;
        this.primitiveSignings.delete(id);
        this.stageCauses.delete(`sign:${id}`);
        if (!code) this.stage(cause, 'signed');
        if (code) this.runFor(cause, this.invoke(this.engine.signFailed, id, code, this.transport.now()));
        else this.runFor(cause, this.invoke(this.engine.signed, id, JSON.stringify(result), this.transport.now()));
      });
    this.primitiveSignings.set(id, pending);
    this.signingJoins.set(id, pending.joined);
    pending.joined.then(() => { if (this.signingJoins.get(id) === pending.joined) this.signingJoins.delete(id); });
    pending.start();
  }

  hostedSigningClock() {
    this.assertHttpHandoffIdle();
    if (this.disposed) throw new EngineDisposedError();
    if (typeof this.engine.hostedSigningClock !== 'function')
      throw new ExchangeError('Hosted signing is unsupported', { kind: 'unsupported', outcome: 'notSent' });
    return JSON.parse(this.invokeHosted(this.engine.hostedSigningClock, this.transport.now()));
  }

  invokeHosted(fn, ...args) {
    this.assertHttpHandoffIdle();
    const output = this.invoke(fn, ...args);
    if (this.disposed) throw this.deadError ?? new EngineDisposedError();
    return output;
  }

  hostedSigning(input) {
    this.assertHttpHandoffIdle();
    if (this.disposed) return Promise.reject(new EngineDisposedError());
    if (typeof this.engine.prepareHostedSigning !== 'function')
      return Promise.reject(new ExchangeError('Hosted signing is unsupported', { kind: 'unsupported', outcome: 'notSent' }));
    let prepared;
    try { prepared = JSON.parse(this.invokeHosted(this.engine.prepareHostedSigning, JSON.stringify(input), this.transport.now())); }
    catch (error) { return Promise.reject(error); }
    const { id, work, deadlineMs } = prepared;
    const duration = deadlineMs - this.transport.now();
    if (!Number.isInteger(id) || id <= 0 || id > 0xffff_ffff || this.hostedSignings.has(id)
      || !Number.isFinite(duration) || duration <= 0 || duration > 5000) {
      this.engine.cancelHostedSigning?.(id);
      return Promise.reject(new ExchangeError('Invalid hosted signing request', { kind: 'invalidInput', outcome: 'notSent' }));
    }
    if (!this.signer) {
      try {
        return Promise.resolve(JSON.parse(this.invokeHosted(this.engine.localHostedSigning, id, this.transport.now())));
      } catch (error) {
        try { this.engine.cancelHostedSigning(id); } catch { /* Preserve the closed native error. */ }
        return Promise.reject(error);
      }
    }
    return new Promise((resolve, reject) => {
      const execute = (context) => this.signer(work, context);
      const pending = this.primitiveSigning(work, duration, execute, (result, code) => {
        this.hostedSignings.delete(id);
        if (code || this.disposed) {
          try { this.engine.cancelHostedSigning(id); } catch { /* Keep a closed signing error. */ }
          reject(new ExchangeError(code === 'timedOut' ? 'Hosted signing timed out' : 'Hosted signing failed', {
            kind: code === 'timedOut' ? 'timeout' : code === 'refused' ? 'invalidInput' : 'internal', outcome: 'notSent',
          }));
          return;
        }
        try { resolve(JSON.parse(this.invokeHosted(this.engine.finishHostedSigning, id, JSON.stringify(result), this.transport.now()))); }
        catch (error) { reject(error); }
      });
      this.hostedSignings.set(id, pending);
      pending.start();
    });
  }

  hostedWalletSigning({ toSign, outputFormat }) {
    this.assertHttpHandoffIdle();
    if (this.disposed) throw new EngineDisposedError();
    // Wallet keys sign only on the device that holds them, never through a callback.
    if (this.signer || typeof this.engine.hostedWalletSigning !== 'function')
      throw new ExchangeError('Hosted wallet signing is unsupported', { kind: 'unsupported', outcome: 'notSent' });
    return this.invokeHosted(this.engine.hostedWalletSigning, toSign, outputFormat, this.transport.now());
  }

  clearSocketFailure(id) {
    this.assertHttpHandoffIdle();
    const pending = this.socketFailures.get(id);
    if (!pending) return;
    this.socketFailures.delete(id);
    this.timers.delete(pending.key);
    try { this.transport.clearTimeout(pending.handle); } catch { /* Its callback is revoked even if cancellation fails. */ }
  }

  socketFailed(id, message) {
    this.assertHttpHandoffIdle();
    const socket = this.sockets.get(id);
    this.sockets.delete(id);
    this.socketClosed(id, 1006);
    try { socket?.close(); } catch { /* The failed transport is already detached. */ }
    if (this.disposed) return;
    this.handlers.onLog?.('error', message);
    if (this.disposed || this.sockets.has(id)) return;
    this.clearSocketFailure(id);
    const pending = { key: Symbol('socketFailure'), handle: undefined };
    this.socketFailures.set(id, pending);
    try {
      // A broken provider cannot recursively reconnect venues without their own backoff.
      const notify = () => {
        if (this.deferHttpNotification(notify)) return;
        if (this.socketFailures.get(id) !== pending) return;
        this.socketFailures.delete(id);
        this.timers.delete(pending.key);
        if (!this.disposed && !this.sockets.has(id)) this.run(this.invoke(this.engine.wsClose, id, 1006, this.transport.now()));
      };
      pending.handle = this.transport.setTimeout(notify, 1_000);
      this.timers.set(pending.key, pending.handle);
    } catch {
      this.socketFailures.delete(id);
      throw new Error('WebSocket failure reporting failed');
    }
  }

  run(actionsJson) {
    this.runActions(actionsJson);
    // Completions for algo commands never reach the host as actions, so arm even when none came back.
    this.armAlgos();
  }

  runActions(actionsJson) {
    this.assertHttpHandoffIdle();
    if (this.disposed || !actionsJson) return;
    let unknown;
    let callbackFailure;
    for (const action of JSON.parse(actionsJson)) {
      if (this.disposed) return;
      // Unknown protocol actions remain errors, but cannot discard the batch's
      // subsequent command completions. Ordinary callbacks still propagate.
      try { this.execute(action); }
      catch (error) {
        if (error instanceof CommandProgressCallbackError || error instanceof CommandDiagnosticCallbackError || error instanceof AccountAuthCallbackError) { callbackFailure ??= error; continue; }
        if (!(error instanceof UnknownEngineActionError)) throw error;
        unknown ??= error;
      }
    }
    if (this.disposed) return;
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
    this.assertHttpHandoffIdle();
    if (this.disposed) return;
    const { engine, transport } = this;
    switch (action?.op) {
      case 'retireAddressSetup':
        if (!this.addressSetupNegotiated) {
          this.dispose();
          throw new EngineDisposedError();
        }
        this.retireAddressSetup(action);
        break;
      case 'sign':
        this.sign(action);
        break;
      case 'wsConnect': {
        const id = action.socket;
        this.clearSocketFailure(id);
        const wsMessageBytes = typeof engine.wsMessageBytes === 'function' ? engine.wsMessageBytes : undefined;
        const wsMessage = typeof engine.wsMessageActions === 'function' ? engine.wsMessageActions : engine.wsMessage;
        if (action.headers?.length && !transport.webSocketHeaders) {
          this.handlers.onLog?.('warning', 'WebSocket upgrade headers unavailable; continuing REST reconciliation');
          const notify = () => {
            if (this.deferHttpNotification(notify)) return;
            if (!this.disposed) this.run(this.invoke(engine.wsClose, id, 1006, transport.now()));
          };
          transport.setTimeout(notify, 0);
          break;
        }
        const stats = this.socketStats[action.channel] ?? this.socketStats.public;
        const record = { channel: action.channel ?? 'public', stats, open: false, initialized: false };
        // Health tracking costs one write per frame; state changes notify once.
        const received = () => {
          stats.lastReceived = Date.now();
          if (record.initialized || !record.open) return;
          record.initialized = true;
          stats.initialized++;
          stats.closeStreak = 0;
          this.socketChanged();
        };
        this.socketRecords.set(id, record);
        let socket;
        let connecting = true;
        try {
          const callbacks = {
            binary: action.binary === true,
            channel: action.channel,
            context: action.context,
            headers: action.headers && Object.fromEntries(action.headers),
            onOpen: () => {
              if (this.deferHttpNotification(callbacks.onOpen, undefined)) return;
              if (this.disposed || !(connecting || (socket && this.sockets.get(id) === socket))) return;
              if (this.socketRecords.get(id) === record && !record.open) {
                record.open = true;
                stats.open++;
                this.socketChanged();
              }
              this.run(this.invoke(engine.wsOpen, id, transport.now()));
            },
            onMessage: (text) => {
              if (this.deferHttpNotification(callbacks.onMessage, text)) return;
              if (this.disposed || (!connecting && (!socket || this.sockets.get(id) !== socket))) return;
              received();
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
              if (this.deferHttpNotification(callbacks.onBinary, undefined, true)) return;
              if (this.disposed || !socket || this.sockets.get(id) !== socket) return;
              received();
              if (engine.wsBinary) this.run(this.invoke(engine.wsBinary, id, data, transport.now()));
              else {
                const active = this.sockets.get(id);
                this.sockets.delete(id);
                this.socketClosed(id, 1003);
                active.close();
                this.run(this.invoke(engine.wsClose, id, 1003, transport.now()));
              }
            },
            onClose: (code) => {
              if (this.deferHttpNotification(callbacks.onClose, code)) return;
              if (this.disposed || !socket || this.sockets.get(id) !== socket) return;
              this.sockets.delete(id);
              this.socketClosed(id, code ?? 1006);
              this.run(this.invoke(engine.wsClose, id, code ?? 1006, transport.now()));
            },
            // A rejectRoute connect with no route left: nothing was dialled, and this
            // is the socket's only terminal (no open or close follows).
            onRouteExhausted: () => {
              if (this.deferHttpNotification(callbacks.onRouteExhausted, undefined)) return;
              if (this.disposed || !socket || this.sockets.get(id) !== socket) return;
              this.sockets.delete(id);
              if (this.socketRecords.get(id) === record) this.socketRecords.delete(id);
              this.run(this.invoke(engine.wsRouteExhausted, id, transport.now()));
            },
          };
          socket = transport.connect(action.url, callbacks);
        } catch {
          connecting = false;
          this.socketFailed(id, 'WebSocket transport connection failed');
          break;
        }
        connecting = false;
        // A synchronous transport may call onOpen before returning its socket.
        if (this.disposed) {
          try { socket.close(); } catch { /* No callbacks may enter a disposed engine. */ }
          break;
        }
        this.sockets.set(id, socket);
        break;
      }
      case 'wsSend':
        try { this.sockets.get(action.socket)?.send(action.text); }
        catch { this.socketFailed(action.socket, 'WebSocket transport send failed'); }
        break;
      case 'wsClose':
        this.clearSocketFailure(action.socket);
        this.sockets.get(action.socket)?.close();
        this.sockets.delete(action.socket);
        this.socketClosed(action.socket, 1000);
        break;
      case 'managedHttp':
        this.managedHttp.execute(action);
        break;
      case 'http': {
        this.stage(this.ownStage(`http:${action.request}`), 'sent');
        void this.request(action);
        break;
      }
      case 'setTimer': {
        const id = action.timer;
        if (this.timers.has(id)) transport.clearTimeout(this.timers.get(id));
        this.timers.delete(id);
        let fired = false;
        let armed = false;
        let handle;
        const notify = () => {
          if (this.deferHttpNotification(notify)) return;
          if (fired || this.disposed || (armed && this.timers.get(id) !== handle)) return;
          fired = true;
          this.timers.delete(id);
          this.run(this.invoke(engine.timer, id, transport.now()));
        };
        handle = transport.setTimeout(notify, Math.max(0, action.at - transport.now()));
        armed = true;
        if (this.disposed || fired) transport.clearTimeout(handle);
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
      case 'startupFailed':
        this.handlers.onStartupFailed?.(action.detail);
        break;
      case 'event': {
        const { level, code, message, fields = {} } = action;
        // Registering the per-call handler before dispatch also covers synchronous events.
        const command = fields?.command;
        const pending = Number.isInteger(command) && command >= 0 && command <= 0xffff_ffff
          ? this.commands.get(command) : undefined;
        if (code === 'history.incomplete' && pending) pending.incomplete = true;
        const event = { level, code, message, fields };
        let callbackFailure;
        try { pending?.onEvent?.(event); }
        catch (cause) { callbackFailure = new CommandDiagnosticCallbackError(cause); }
        if (this.handlers.onEvent) this.handlers.onEvent(event);
        else if (level !== 'debug') this.handlers.onLog?.(level === 'warn' ? 'warning' : level, message);
        // A broken per-command observer cannot skip a later native completion.
        if (callbackFailure) throw callbackFailure;
        break;
      }
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
        this.stage(action.command, 'reply');
        const pending = this.commands.get(action.command);
        if (!pending) {
          // Late definitive results close the receipt stream without a tombstone.
          // Malformed/duplicate results still cannot change an earlier outcome.
          try { if (action.error === undefined && action.result !== undefined) JSON.parse(action.result); }
          catch { break; }
          this.commandProgress(action.command, [], true, true);
          break;
        }
        if (action.error !== undefined && action.detail && typeof action.detail.message === 'string') {
          const error = new ExchangeError(action.detail.message, action.detail);
          commandReceiptSnapshot(error, pending.identity);
          this.settleCommand(action.command, error, true);
        }
        else if (action.error !== undefined) {
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
          // Optional and non-enumerable: rows stay a plain array for existing callers.
          if (pending.incomplete && Array.isArray(result)) Object.defineProperty(result, 'incomplete', { value: true, writable: true, configurable: true });
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
    const reader = stream.getReader();
    const parts = [];
    let length = 0;
    let complete = false;
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) { complete = true; break; }
        length += value.byteLength;
        if (length > MAX_DECOMPRESSED_BYTES) throw new Error('Decompressed WebSocket message exceeds limit');
        parts.push(value);
      }
    } finally {
      if (!complete) await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
    const decoded = new Uint8Array(length);
    let offset = 0;
    for (const part of parts) { decoded.set(part, offset); offset += part.byteLength; }
    return new TextDecoder('utf-8', { fatal: true }).decode(decoded);
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
  prepareHttp(request) { return prepareFetch(request); },
  async request(request) { return prepareFetch(request).bind(request).dispatch(); },
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (handle) => clearTimeout(handle),
};
};
