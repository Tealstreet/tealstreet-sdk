import { createHmac, timingSafeEqual } from 'node:crypto';
import { canonicalUrl as strictUrl, requestKey, callerIdsOf, isWriteRequest, selectResponse, subscriptionKeys, subscriptionMessage } from './requests.mjs';
import { parseRequestJson } from './request-json.mjs';
import { Responses } from './responses.mjs';
import { setTimeout as realTimeout, clearTimeout as realClearTimeout } from 'node:timers';
import { LOADED_FLAG } from '../hosts/wasm/js/wire.mjs';
export class ReplayTimeout extends Error {}
export const withTimeout = async (promise, label, ms = 30000) => {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = realTimeout(() => reject(new ReplayTimeout(`replay timeout: ${label}`)), ms);
      }),
    ]);
  } finally {
    realClearTimeout(timer);
  }
};
export const within = (promise, ms, label) => withTimeout(promise, label, ms);
export const canonicalUrl = (url, callerIds = new Set(), venue = {}) => strictUrl(venue.requestUrl?.(url) ?? url, callerIds, venue);
export const canonicalBody = (body, callerIds = new Set(), venue = {}) => {
  const url = 'https://api.mexc.com/api/v1/private/order/cancel';
  const prefix = requestKey('POST', url, undefined, callerIds, venue);
  return requestKey('POST', url, body, callerIds, venue).slice(prefix.length + 1);
};
export const stable = (value) =>
  Array.isArray(value)
    ? value.map(stable)
    : value && typeof value === 'object'
      ? Object.fromEntries(
          Object.keys(value)
            .sort()
            .map((key) => [key, stable(value[key])]),
        )
      : value;
export const sameCommandResults = (a, b) => JSON.stringify(stable(a)) === JSON.stringify(stable(b));
export const deliverSocketFrame = (socket, text, audit, validate) => {
  if (!socket.auditInSocket) audit?.received(socket.url, text, socket.generation);
  validate?.(socket, text);
  socket.onMessage(text);
};
export const replayStoreReady = (loaded, isPrivate) => {
  const publicMask = LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS;
  return (
    (loaded &
      (isPrivate ? publicMask | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS : publicMask)) ===
    (isPrivate ? publicMask | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS : publicMask)
  );
};

export async function bodyBytes(body) {
  if (body instanceof Uint8Array) return body;
  const chunks = [];
  let length = 0;
  for await (const chunk of body) {
    chunks.push(chunk);
    length += chunk.length;
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}

export class ReplayClock {
  constructor(epoch, freezeStartup = false, continueStartup = false) {
    this.epoch = epoch;
    this.time = epoch;
    this.active = false;
    this.freezeStartup = freezeStartup;
    this.continueStartup = continueStartup;
    this.timers = new Map();
    this.original = { now: Date.now, setTimeout, clearTimeout, setInterval, clearInterval };
  }
  install() {
    Date.now = () => (this.active || this.freezeStartup ? this.time : this.original.now());
    globalThis.setTimeout = (fn, ms = 0, ...args) => this.schedule(fn, ms, args, false);
    globalThis.setInterval = (fn, ms = 0, ...args) => this.schedule(fn, ms, args, true);
    globalThis.clearTimeout = globalThis.clearInterval = (timer) => {
      if (this.timers.has(timer)) {
        this.original.clearTimeout(timer.handle);
        this.timers.delete(timer);
      } else this.original.clearTimeout(timer);
    };
  }
  schedule(fn, ms, args, repeat) {
    const timer = {
      fn,
      args,
      // Node runs timers on whole milliseconds; a fractional backoff would
      // otherwise make Date.now() fractional, which no real clock returns.
      delay: Math.max(repeat ? 1 : 0, Math.trunc(Number(ms)) || 0),
      repeat,
      ref() {
        return this;
      },
      unref() {
        return this;
      },
    };
    timer.at = Date.now() + timer.delay;
    this.timers.set(timer, timer);
    if (!this.active) this.arm(timer);
    return timer;
  }
  arm(timer) {
    const generation = (timer.generation ?? 0) + 1;
    timer.generation = generation;
    timer.handle = this.original.setTimeout(() => {
      if (timer.generation === generation) return this.fire(timer);
    }, timer.delay);
  }
  fire(timer) {
    if (!this.timers.has(timer)) return;
    if (this.freezeStartup && !this.active) {
      // Wall-time stalls can queue a later native wake before an earlier
      // logical child timer even exists. Drain by logical deadline, including
      // timers created by callbacks, rather than trusting native queue order.
      this.startupTarget = Math.max(this.startupTarget ?? this.time, timer.at);
      if (!this.drainingStartup) {
        this.drainingStartup = true;
        this.startupDrain = this.drainStartup();
      }
      return this.startupDrain;
    }
    this.fireTimer(timer);
  }
  async drainStartup() {
    try {
      while (!this.active) {
        let next;
        for (const pending of this.timers.values())
          if (pending.at <= this.startupTarget && (!next || pending.at < next.at)) next = pending;
        if (!next) break;
        this.original.clearTimeout(next.handle);
        next.generation = (next.generation ?? 0) + 1;
        this.fireTimer(next);
        // Native timer callbacks also drain their Promise continuations before
        // another callback. A token queue may schedule its child timer there.
        await new Promise(setImmediate);
      }
    } finally {
      this.drainingStartup = false;
      this.startupTarget = undefined;
      this.startupDrain = undefined;
    }
  }
  fireTimer(timer) {
    if (!this.timers.has(timer)) return;
    // Frozen startup still has timed prerequisites (rate limits, paced request
    // queues). Advance on their scheduled logical deadline, never the wall
    // time at which auditing allowed the callback to run.
    if (this.freezeStartup && !this.active) this.time = Math.max(this.time, timer.at);
    if (timer.repeat) {
      timer.at = Date.now() + timer.delay;
      if (!this.active) this.arm(timer);
    } else this.timers.delete(timer);
    timer.fn(...timer.args);
  }
  start() {
    const wall = this.freezeStartup ? this.time : this.original.now();
    // Tape time 0 is the end of frozen startup. Rewinding Date.now() there would
    // stall any pacer that stored an absolute deadline during startup.
    if (this.freezeStartup && this.continueStartup) this.epoch = wall;
    for (const timer of this.timers.values()) {
      this.original.clearTimeout(timer.handle);
      timer.generation = (timer.generation ?? 0) + 1;
      timer.at = this.epoch + Math.max(0, timer.at - wall);
    }
    this.active = true;
    this.time = this.epoch;
  }
  async advance(t) {
    const target = Math.max(this.time, this.epoch + t);
    for (;;) {
      let next;
      for (const timer of this.timers.values()) if (timer.at <= target && (!next || timer.at < next.at)) next = timer;
      if (!next) break;
      this.time = next.at;
      this.fire(next);
      await new Promise(setImmediate);
    }
    this.time = target;
  }
  async wait(promise, label, timeoutMs, { maxSteps = Infinity, maxIdleSteps = Infinity, progress } = {}) {
    let done = false;
    let steps = 0;
    let idleSteps = 0;
    let previousProgress = progress?.();
    const result = Promise.resolve(promise).finally(() => {
      done = true;
    });
    const pump = (async () => {
      while (!done) {
        await new Promise(setImmediate);
        if (done) break;
        if (++steps > maxSteps) throw new ReplayTimeout(`replay timeout: ${label}; virtual step budget exceeded`);
        const currentProgress = progress?.();
        if (currentProgress !== previousProgress) {
          previousProgress = currentProgress;
          idleSteps = 0;
        }
        let next;
        for (const timer of this.timers.values()) if (!next || timer.at < next.at) next = timer;
        if (next) {
          idleSteps = 0;
          this.time = Math.max(this.time, next.at);
          this.fire(next);
        } else if (++idleSteps > maxIdleSteps)
          throw new ReplayTimeout(`replay timeout: ${label}; idle step budget exceeded`);
      }
      return result;
    })();
    try {
      return await withTimeout(Promise.race([result, pump]), label, timeoutMs);
    } finally {
      done = true;
    }
  }
  restore() {
    for (const timer of this.timers.values()) this.original.clearTimeout(timer.handle);
    this.timers.clear();
    Date.now = this.original.now;
    for (const name of ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'])
      globalThis[name] = this.original[name];
  }
}

// Fixed tape ordinals, never wall-time or implementation progress, select the
// same intermediate financial states in both replays. Command boundaries are
// always included so a later final snapshot cannot hide a wrong mutation.
export const checkpointOrdinals = (frames) => new Set([
  0,
  ...[1, 2, 3, 4].map((quarter) => Math.ceil(frames.length * quarter / 4)),
  ...frames.flatMap((frame, index) => frame.type === 'command' ? [index + 1] : []),
]);

// Derive the complete required capture schedule from the input tape, never
// from either replay output. Retain original record identities across selected
// implementation plans, TS request-body aliases and bootstrap/venue filters.
export const expectedCheckpointPlan = (records, venue = {}, impl) => {
  if (impl !== undefined && !['ts', 'rust'].includes(impl)) throw new Error('Unknown replay implementation');
  const selected = records.flatMap((record, recordIndex) => {
    if (record.impl !== undefined) {
      if (!['ts', 'rust'].includes(record.impl)) throw new Error('Unknown tape record implementation');
      if (impl === undefined) throw new Error('Tagged tape records require a replay implementation');
      if (record.impl !== impl) return [];
    }
    let row = record;
    if (record.effectiveAt !== undefined && record.availableAfter === undefined) row = { ...row, availableAfter: record.effectiveAt };
    if (impl === 'ts' && record.tsRequestBody !== undefined) row = { ...row, requestBody: record.tsRequestBody };
    return [{ record: row, recordIndex }];
  });
  const liveSmoke = selected.some(({ record }) => record.type === 'meta' && record.liveSmoke === true);
  const frames = selected.filter(({ record }) => (
    ['ws', 'command', 'close', 'open', 'wait'].includes(record.type) || (venue.polling === true && record.type === 'http')
  ) && !venue.skipFrame?.(record)).filter(({ record }) => !(
    venue.bootstrapFrame?.(record) || (liveSmoke && ['open', 'close', 'ws'].includes(record.type) && record.bootstrap === true)
  ));
  const required = checkpointOrdinals(frames.map(({ record }) => record));
  return [
    { ordinal: 0, recordIndex: -1, t: 0 },
    ...frames.flatMap(({ record, recordIndex }, index) => required.has(index + 1)
      ? [{ ordinal: index + 1, recordIndex, t: record.t ?? 0 }] : []),
  ];
};

export class Tape {
  constructor(records, venue = {}, isPrivate = venue.isPrivate ?? false, timeoutMs = 30000, impl, books = [], options, settings = {}) {
    this.auditEnabled = settings.performance !== true;
    // Minimal receipts share the same recorder in both modes. They retain
    // routing-computed keys and send references, with all normalization after
    // performance sampling. Their small recording cost remains in both runs.
    this.receiptRequests = [];
    this.receiptSends = [];
    const originalIndices = new Map(records.map((record, index) => [record, index]));
    if (impl !== undefined && !['ts', 'rust'].includes(impl)) throw new Error('Unknown replay implementation');
    records = records.filter((record) => {
      if (record.impl === undefined) return true;
      if (!['ts', 'rust'].includes(record.impl)) throw new Error('Unknown tape record implementation');
      if (impl === undefined) throw new Error('Tagged tape records require a replay implementation');
      return record.impl === impl;
    });
    this.records =
      impl === 'ts'
        ? records.map((record) =>
            record.tsRequestBody === undefined ? record : { ...record, requestBody: record.tsRequestBody },
          )
        : records;
    this.recordIndices = new Map(this.records.map((record, index) => [record, originalIndices.get(records[index])]));
    this.venue = venue;
    this.liveSmoke = records.some((record) => record.type === 'meta' && record.liveSmoke === true);
    this.bootstrapFrame = (frame) => venue.bootstrapFrame?.(frame) || (this.liveSmoke && ['open', 'close', 'ws'].includes(frame.type) && frame.bootstrap === true);
    this.isPrivate = isPrivate;
    this.timeoutMs = timeoutMs;
    this.options = options ?? {
      key: 'replay-key',
      secret: 'replay-secret',
      passphrase: 'replay-passphrase',
      ...venue.replayCredentials,
    };
    this.callerIds = callerIdsOf(records);
    this.socketUrlKeys = new Map();
    this.validateRequest = this.auditEnabled
      ? venue.createRequestValidator?.(this.options, {
        callerIds: this.callerIds,
        getCommandIndex: () => this.commandIndex,
        getExpectedWrite: () => ({ key: this.expectedWrites[0], record: this.responses.queues.get(this.expectedWrites[0])?.[0] }),
      }) ?? venue.validateRequest
      : undefined;
    this.bookSymbols = books;
    this.bitget = venue.rust?.startsWith('bitget') ?? false;
    this.generated = new Set();
    this.generatedAliases = new Map();
    this.identityOptions = this.bitget
      ? { ...venue, requestBody: (body, context) => this.bitgetBody(body, context) }
      : venue;
    this.keyFor = (method, url, body) => requestKey(method, url, body, this.callerIds, this.identityOptions);
    this.wireAliases = new Map();
    if (impl === 'ts')
      for (const record of records)
        if (record.tsRequestBody !== undefined) {
          this.wireAliases.set(
            this.keyFor(record.method, record.url, record.tsRequestBody),
            this.keyFor(record.method, record.url, record.requestBody),
          );
        }
    const writeRecords = records.filter(
      (record) =>
        record.type === 'http' &&
        (venue.isWrite?.(record.method, record.url) ?? isWriteRequest(record.method, record.url)),
    );
    this.expectedWrites = writeRecords.map((record) => this.keyFor(record.method, record.url, record.requestBody));
    // A read staged after command N shows its outcome. Orderly's final
    // snapshots wait for every staged write; other venues retain first-ACK
    // release. Command settlement remains a separate scheduling fallback.
    // Unstaged writes cannot be attributed to a command and gate nothing.
    this.unacknowledgedStages = new Set(writeRecords.map((record) => record.afterCommand ?? 0).filter((stage) => stage > 0));
    this.pendingStageWrites = venue.waitForAllStageWrites ? new Map() : undefined;
    if (this.pendingStageWrites) for (const record of writeRecords) {
      const stage = record.afterCommand ?? 0;
      if (stage > 0) this.pendingStageWrites.set(stage, (this.pendingStageWrites.get(stage) ?? 0) + 1);
    }
    this.settledCommands = 0;
    this.publicTickers = this.venue.publicTickers?.(this.records) ?? [];
    this.requestAudit = this.auditEnabled ? this.venue.createRequestAudit?.(impl, { isPrivate: this.isPrivate }) : undefined;
    this.audit = this.auditEnabled ? this.venue.createSocketAudit?.(impl, {
      isPrivate: this.isPrivate,
      books: this.bookSymbols,
      tickers: this.publicTickers,
      requestAudit: this.requestAudit,
      records: this.records,
    }) : undefined;
    this.responses = new Responses(
      this.records,
      this.keyFor,
      this.auditEnabled ? this.venue.checkRequest : undefined,
      (method, url) => this.venue.isWrite?.(method, url) ?? isWriteRequest(method, url),
      true,
      impl,
    );
    if (this.venue.polling === true) {
      this.selectedPollingResponses = new Set();
      this.polledRequestKeys = new Set();
    }
    this.frames = this.records.filter((r) => (r.type === 'ws' || r.type === 'command' || r.type === 'close' || r.type === 'open' || r.type === 'wait' || (this.venue.polling === true && r.type === 'http')) && !this.venue.skipFrame?.(r));
    if (this.frames.some((frame) => frame.type === 'wait' && (!Number.isFinite(frame.t) || frame.t < 0)))
      throw new Error('Invalid replay wait timestamp');
    this.lifecycleStreams = new Map();
    this.lifecycleSockets = new Map();
    if (this.venue.subscriptionRouting) for (const frame of this.frames) {
      if (frame.type !== 'ws' || frame.socket === undefined) continue;
      const data = frame.binary === undefined ? frame.text : Uint8Array.from(Buffer.from(frame.binary, 'base64'));
      const stream = this.venue.streamOf?.(data);
      if (!stream) continue;
      const key = `${this.socketUrlKey(frame.url)} ${frame.socket}`;
      if (!this.lifecycleStreams.has(key)) this.lifecycleStreams.set(key, new Set());
      this.lifecycleStreams.get(key).add(stream);
    }
    this.startupDelivered = new Set();
    this.earlyDelivered = new Set();

    this.open = new Map();
    this.waiters = [];
    this.commandIndex = 0;
    this.time = 0;
    this.subscriptions = new Set();
    // Retain expected keys/backing storage across both memory samples: deleting
    // startup-only harness state would subtract it from workload retained memory.
    this.startupSubscriptions = new Map(
      Array.from(this.venue.startupSubscriptions?.(this.records, this.bookSymbols) || [], (key) => [key, false]),
    );
    this.pendingStartupSubscriptions = this.startupSubscriptions.size;
    this.generations = new Map();
    this.recordedSocketRequests = new Map();
    this.recordedSocketQueues = new Map();
    for (const record of records.filter((r) => r.type === 'ws-send')) {
      let message;
      try { message = JSON.parse(record.text); } catch { continue; /* Plain-text heartbeat. */ }
      const request = venue.socketRequest?.(message);
      if (!request) continue;
      const key = `${this.socketUrlKey(record.url)} ${record.socket ?? 0} ${request.id}`;
      if (this.recordedSocketRequests.has(key)) throw new Error(`duplicate taped websocket request ${key}`);
      this.recordedSocketRequests.set(key, request.key);
      const queueKey = `${this.socketUrlKey(record.url)} ${record.socket ?? 0} ${request.key}`;
      if (!this.recordedSocketQueues.has(queueKey)) this.recordedSocketQueues.set(queueKey, []);
      this.recordedSocketQueues.get(queueKey).push(request.id);
    }
  }

  socketUrlKey(url) {
    // Custom rules can close over mutable state. Only memoize default identities.
    if (typeof url !== 'string' || this.callerIds.size || this.venue.requestUrl
      || this.venue.volatileQueryFields || this.venue.generatedQueryId)
      return canonicalUrl(url, this.callerIds, this.venue);
    let key = this.socketUrlKeys.get(url);
    if (key === undefined) {
      key = canonicalUrl(url, this.callerIds, this.venue);
      this.socketUrlKeys.set(url, key);
    }
    return key;
  }

  get unserved() {
    return this.responses.unserved;
  }

  mintedField(key, url) {
    const path = new URL(url).pathname;
    return key === 'newClientOid'
      ? /\/modify-order$/.test(path)
      : key === 'clientOid' &&
          /\/(?:batch-place-order|place-order|place-tpsl-order|place-plan-order|place-strategy-order)$/.test(path);
  }

  bitgetBody(body, { url }) {
    const visit = (value) => {
      if (JSON.isRawJSON(value)) return value;
      if (Array.isArray(value)) return value.map(visit);
      if (!value || typeof value !== 'object') return value;
      return Object.fromEntries(
        Object.entries(value).map(([key, item]) => [
          key,
          this.mintedField(key, url) && typeof item === 'string' && item.length && !this.callerIds.has(item)
            ? '<generated>'
            : key === 'clientOid' && typeof item === 'string'
              ? (this.generatedAliases.get(item) ?? item)
              : visit(item),
        ]),
      );
    };
    return visit(body);
  }

  validateBitgetMatch(record, method, url, body) {
    if (!this.bitget || method === 'GET') return;
    const decode = (value) => {
      if (ArrayBuffer.isView(value) || value instanceof ArrayBuffer) value = new TextDecoder().decode(value);
      return typeof value === 'string' ? parseRequestJson(value) : value;
    };
    const sent = decode(body);
    const ids = [];
    const visit = (value) => {
      if (!value || typeof value !== 'object') return;
      for (const [key, item] of Object.entries(value)) {
        if (this.mintedField(key, url) && (key === 'newClientOid' || !this.callerIds.has(item))) {
          if (
            typeof item !== 'string' ||
            !/^[.A-Za-z0-9:/_-]{1,32}$/.test(item) ||
            (key === 'newClientOid' && this.callerIds.has(item))
          )
            throw new Error(`invalid generated ${key}`);
          ids.push(item);
        } else visit(item);
      }
    };
    if (this.auditEnabled) {
      visit(sent);
      const retry = String(JSON.parse(record.body).code) === '40009';
      if (!retry) {
        if (new Set(ids).size !== ids.length) throw new Error('generated client ID reused within batch');
        for (const id of ids) if (this.generated.has(id)) throw new Error(`generated client ID reused: ${id}`);
        for (const id of ids) this.generated.add(id);
      }
    }
    // Alias binding is transport simulation, required even without auditing.
    const bind = (expected, actual) => {
      if (!expected || !actual || typeof expected !== 'object' || typeof actual !== 'object') return;
      for (const key of Object.keys(expected)) {
        if (
          this.mintedField(key, url) &&
          typeof expected[key] === 'string' &&
          typeof actual[key] === 'string' &&
          !this.callerIds.has(actual[key])
        )
          this.generatedAliases.set(actual[key], expected[key]);
        else bind(expected[key], actual[key]);
      }
    };
    bind(decode(record.requestBody), sent);
  }

  topicKey(topic) {
    return `${topic.instType ?? ''}|${topic.channel ?? topic.topic}|${topic.instId ?? topic.symbol ?? ''}`;
  }

  verifySignature(verify, value, label) {
    try {
      if (verify(value)) return;
      throw new Error(label);
    } catch (error) {
      // Persist both a rejected signature and a verifier that cannot parse it.
      this.invalidSignatures++;
      throw error;
    }
  }

  respond(method, url, body, headers) {
    // Stateful validators may return a commit callback. Run it only after a
    // response is eligible and matched; rejected/premature attempts bind no IDs.
    let commitValidation;
    try {
      commitValidation = this.validateRequest?.({ method, url, body, headers, callerIds: this.callerIds });
      this.requestAudit?.validate({ method, url, body, headers });
    } catch (error) {
      this.invalidSignatures++;
      this.pollingFence?.reject(error);
      throw error;
    }
    if (this.auditEnabled && this.isPrivate && this.venue.verifyRequest) {
      this.verifySignature(
        this.venue.verifyRequest,
        { method, url, body, headers },
        `invalid replay signature: ${method} ${url}`,
      );
    }
    const actualKey = this.keyFor(method, url, body);
    const key = this.wireAliases.get(actualKey) ?? actualKey;
    this.receiptRequests.push([key, this.responses.isWrite(method, url)]);
    if (this.auditEnabled) {
      this.requests.add(key);
      if (this.responses.isWrite(method, url)) this.writes.push(key);
    }
    if (this.responses.isWrite(method, url) && this.venue.compareWriteOrder && this.expectedWrites[0] !== key) {
      if (process.env.RUST_REQUEST_TRACE) console.error('unexpected write', key, 'expected', this.expectedWrites[0]);
      this.responses.unserved++;
      this.pollingFence?.reject(new Error('Polling replay sent an unexpected HTTP mutation'));
      return { status: 599, body: '' };
    }
    const elapsed = this.replayClock?.active ? this.replayClock.time - this.replayClock.epoch : this.time;
    const stage = this.responses.isWrite(method, url) ? this.commandIndex : this.readStage();
    const unservedBefore = this.responses.unserved;
    let response;
    try {
      response = typeof commitValidation === 'function' || this.bitget || (this.auditEnabled && this.venue.validateMatchedRequest) || this.selectedPollingResponses
        ? this.responses.respond(method, url, body, stage, (record) => {
          this.validateBitgetMatch(record, method, url, body);
          if (this.auditEnabled) this.venue.validateMatchedRequest?.(record, { method, url, body, headers, commandIndex: this.commandIndex });
          if (typeof commitValidation === 'function') commitValidation(record);
          if (this.selectedPollingResponses && !this.responses.isWrite(method, url)) {
            this.polledRequestKeys.add(actualKey);
            this.selectedPollingResponses.add(record);
            this.pollingFence?.matched(actualKey, record);
          }
        }, elapsed, headers)
        : this.responses.respond(method, url, body, stage, elapsed, headers);
    } catch (error) {
      this.pollingFence?.reject(error);
      throw error;
    }
    if (this.responses.unserved !== unservedBefore) this.pollingFence?.reject(new Error('Polling replay sent an unmatched HTTP request'));
    const matchedWrite = this.responses.isWrite(method, url) && response.status !== 599;
    const ackStage = matchedWrite ? this.responses.served.afterCommand : undefined;
    if (matchedWrite) {
      this.expectedWrites.shift();
      if (!this.pendingStageWrites) this.unacknowledgedStages.delete(ackStage);
      this.deliverBeforeAck(ackStage);
    }
    try {
      this.requestAudit?.response({ method, url, body, headers, response });
    } catch (error) {
      this.invalidSignatures++;
      throw error;
    }
    if (matchedWrite && this.pendingStageWrites) {
      const pending = this.pendingStageWrites.get(ackStage);
      if (pending > 1) this.pendingStageWrites.set(ackStage, pending - 1);
      else {
        this.pendingStageWrites.delete(ackStage);
        this.unacknowledgedStages.delete(ackStage);
      }
    }
    return response;
  }

  async finishPolling(clock) {
    if (!this.selectedPollingResponses) return;
    if (this.unserved) throw new Error('Polling replay contains unmatched HTTP requests');
    const targets = new Map();
    for (const [key, queue] of this.responses.queues) {
      if (!this.polledRequestKeys.has(key) || !queue.length || this.responses.isWrite(queue[0].method, queue[0].url)) continue;
      const target = selectResponse([...queue], this.time, { commandIndex: this.readStage() });
      if (target && (target.availableAt !== undefined || target.effectiveAt !== undefined || target.availableAfter !== undefined)
          && !this.selectedPollingResponses.has(target)) targets.set(key, target);
    }
    if (!targets.size) return;
    const label = `polling final HTTP anchors (${[...targets.values()].map((record) => `${record.method} ${new URL(record.url).pathname}`).join(', ')})`;
    if (!clock) throw new Error(`${label} require a replay clock`);
    let resolve;
    let reject;
    const pending = new Promise((accept, fail) => { resolve = accept; reject = fail; });
    const attempts = new Map();
    this.pollingFence = {
      reject,
      matched(key, record) {
        const target = targets.get(key);
        if (!target) return;
        if (record === target) {
          targets.delete(key);
          if (!targets.size) resolve();
        } else {
          const count = (attempts.get(key) ?? 0) + 1;
          attempts.set(key, count);
          if (count >= 2) reject(new Error(`${label} were superseded before a natural poll selected them`));
        }
      },
    };
    try {
      await clock.wait(pending, label, this.timeoutMs, {
        maxSteps: 4096 * (targets.size + 1), maxIdleSteps: 64, progress: () => targets.size,
      });
    } finally {
      this.pollingFence = undefined;
    }
  }

  // A venue often pushes a write's order event before its REST acknowledgement.
  // A frame marked beforeAck reaches its socket after the write is served but
  // before the client consumes the acknowledgement (a microtask ahead of it).
  deliverBeforeAck(stage) {
    if (!(stage > 0)) return;
    for (const frame of this.frames) {
      if (!frame.beforeAck || frame.afterCommand !== stage || this.earlyDelivered.has(frame)) continue;
      const [socket] = this.sockets(frame.url, frame.socket);
      if (!socket) continue;
      this.earlyDelivered.add(frame);
      queueMicrotask(() => {
        try { this.deliver(socket, frame.text); } catch (error) { this.earlyFailure ??= error; }
      });
    }
  }

  readStage() {
    const command = this.commandIndex;
    return command > this.settledCommands && this.unacknowledgedStages.has(command) ? command - 1 : command;
  }

  connect(url, onOpen, onMessage, onClose) {
    const key = this.socketUrlKey(url);
    const generation = (this.generations.get(key) ?? 0) + 1;
    this.generations.set(key, generation);
    const socket = {
      url, ordinal: generation - 1, generation, onClose, closed: false, loginVerified: false,
      topics: new Map(), subscriptions: new Set(), requests: new Map(), requestIds: new Set(),
      requestCounts: new Map(), mappedRequests: new Map(), responseIds: new Set(),
      auditInSocket: Boolean(this.venue.socketResponse),
      onMessage: this.venue.socketResponse ? (text) => {
        const correlated = this.correlateSocketResponse(socket, text);
        this.audit?.received(url, correlated, generation);
        onMessage(correlated);
      } : onMessage,
    };
    this.audit?.connected?.(url, generation);
    setImmediate(() => {
      if (socket.closed) return;
      // Bybit adds a generated requestId to each connection URL.
      if (!this.open.has(key)) this.open.set(key, new Set());
      this.open.get(key).add(socket);
      onOpen();
      // Bootstrap only the first recorded frame for this exact connection. A
      // business frame or close before welcome must keep startup fail-closed.
      if (!socket.closed && !this.playing && this.venue.startupMessage) {
        const frame = this.frames.find((record) => record.type !== 'open' && record.type !== 'command' && record.type !== 'wait' && record.type !== 'http'
          && !this.startupDelivered.has(record) && this.socketUrlKey(record.url) === key
          && (record.socket === undefined || record.socket === socket.ordinal));
        if (frame?.type === 'ws' && this.venue.startupMessage(frame)) {
          this.startupDelivered.add(frame);
          this.deliver(socket, frame.text);
        }
      }
      this.waiters = this.waiters.filter((waiter) => !waiter());
    });
    return socket;
  }

  send(socket, text) {
    this.receiptSends.push({ url: socket.url, text, generation: socket.generation });
    try { if (this.auditEnabled) this.venue.validateSocketMessage?.(socket, text); }
    catch(error) { this.invalidSignatures++; throw error; }
    let message;
    try {
      message = JSON.parse(text);
    } catch {
      /* Plain-text heartbeat. */
    }
    if (this.venue.bootstrapPrivateFromTape && message?.op && message?.req_id)
      socket.requests.set(message.op, message.req_id);
    if (this.auditEnabled && message && this.isPrivate && this.venue.verifySubscribe) {
      this.verifySignature(this.venue.verifySubscribe, message, 'invalid replay websocket signature');
    }
    if (message) {
      const request = this.venue.socketRequest?.(message);
      if (request) {
        if (this.auditEnabled && socket.requestIds.has(request.id)) throw new Error(`duplicate websocket request ${request.key}`);
        if (this.recordedSocketRequests.size || this.venue.requireSocketRequestRecords) {
          const queueKey = `${this.socketUrlKey(socket.url)} ${socket.ordinal} ${request.key}`;
          const count = socket.requestCounts.get(request.key) ?? 0;
          const recordedId = this.recordedSocketQueues.get(queueKey)?.[count];
          if (recordedId === undefined) throw new Error(`unrecorded websocket request ${request.key}`);
          socket.requestCounts.set(request.key, count + 1);
          socket.mappedRequests.set(recordedId, request.id);
        }
        socket.requests.set(request.id, request.key);
        if (this.auditEnabled) {
          socket.requestIds.add(request.id);
          this.sentRequests.add(JSON.stringify(stable({ url: socket.url, request: request.key })));
        }
      }
      if (message.event || this.venue.family === 'coinbase') {
        const subscription = subscriptionMessage(message);
        for (const topic of subscription?.topics ?? []) {
          if (typeof topic !== 'string' || !topic) throw new Error('invalid websocket subscription topic');
          if (subscription.op === 'subscribe') socket.subscriptions.add(topic);
          else socket.subscriptions.delete(topic);
        }
      }
    }
    if (Array.isArray(message?.params)) for (const topic of message.params) {
      if (message.method === 'SUBSCRIBE') socket.subscriptions.add(topic);
      if (message.method === 'UNSUBSCRIBE') socket.subscriptions.delete(topic);
    }
    if (typeof message?.params?.channel === 'string') {
      if (message.method === 'subscribe') socket.subscriptions.add(message.params.channel);
      if (message.method === 'unsubscribe') socket.subscriptions.delete(message.params.channel);
    }
    this.venue.onSocketSend?.(socket, text);
    // Authentication may classify a shared-URL socket after its open callback.
    this.waiters = this.waiters.filter((waiter) => !waiter());
    if (this.auditEnabled) this.sent(socket.url, text);
    this.audit?.sent(socket.url, text, socket.generation);
    for (const reply of this.venue.socketReplies?.(socket.url, message) ?? []) {
      setImmediate(() => { if (!socket.closed) this.deliver(socket, reply); });
    }
    const compared = this.venue.marketFilter && Array.isArray(message?.params)
      ? { ...message, params: message.params.filter((topic) => topic.startsWith('account.') || this.venue.marketFilter({ symbol: topic.split('.').at(-1) })) } : text;
    if (this.auditEnabled || this.pendingStartupSubscriptions) {
      for (const key of subscriptionKeys(socket.url, compared, this.callerIds, this.venue)) {
        if (this.auditEnabled) this.subscriptions.add(this.venue.subscriptionGenerations ? `${socket.generation} ${key}` : key);
        if (this.startupSubscriptions.get(key) === false) {
          this.startupSubscriptions.set(key, true);
          this.pendingStartupSubscriptions--;
        }
      }
    }
    if (this.venue.pingReply) {
      const reply = this.venue.pingReply(text);
      if (reply) setImmediate(() => { if (!socket.closed) this.deliver(socket, reply); });
    }
    if (this.venue.respondToPing && text === 'ping') {
      setImmediate(() => {
        if (!socket.closed) this.deliver(socket, 'pong');
      });
      return;
    }
    if (this.venue.respondToJsonPing && message?.type === 'ping') {
      setImmediate(() => {
        if (!socket.closed) this.deliver(socket, JSON.stringify({ type: 'pong', id: message.id }));
      });
      return;
    }
    // OKX's opt-in auth validation also uses its v5 subscription argument model.
    const validateTopics = this.bitget || (this.venue.validateSubscriptions ?? this.venue.validateLogin);
    if (!this.venue.validateLogin && !validateTopics) return;
    if (text === 'ping' || text === 'pong') return;
    message = JSON.parse(text);
    if (validateTopics && ['subscribe', 'unsubscribe'].includes(message.op)) {
      if (!Array.isArray(message.args)) {
        if (this.auditEnabled) throw new Error(`Invalid subscription on ${socket.url}`);
        return;
      }
      for (const topic of message.args) {
        if (
          this.auditEnabled && (!topic ||
          typeof (topic.channel ?? (this.bitget ? topic.topic : undefined)) !== 'string' ||
          !(topic.channel ?? topic.topic))
        )
          throw new Error(`Invalid subscription topic on ${socket.url}`);
        const key = this.bitget
          ? this.topicKey(topic)
          : JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a], [b]) => a.localeCompare(b))));
        if (message.op === 'subscribe') socket.topics.set(key, topic);
        else socket.topics.delete(key);
      }
      return;
    }
    if (!this.venue.validateLogin || !/\/(private|business)$/.test(new URL(socket.url).pathname)) return;
    if (message.op !== 'login') return;
    if (!this.auditEnabled) {
      // Retain authentication state transitions while excluding verifier work.
      socket.loginVerified = true;
      return;
    }
    const arg = message.args?.length === 1 ? message.args[0] : undefined;
    const timestamp = String(arg?.timestamp ?? '');
    if (
      arg?.apiKey !== this.options.key ||
      arg?.passphrase !== this.options.passphrase ||
      !/^\d+$/.test(timestamp) ||
      !Number.isSafeInteger(Number(timestamp)) ||
      Math.abs(Number(timestamp) - Date.now() / 1000) > 30
    ) {
      throw new Error(`Invalid OKX login credentials/timestamp on ${socket.url}`);
    }
    const expected = createHmac('sha256', this.options.secret)
      .update(`${timestamp}GET/users/self/verify`)
      .digest('base64');
    const supplied = Buffer.from(String(arg.sign ?? ''));
    const expectedBytes = Buffer.from(expected);
    if (supplied.length !== expectedBytes.length || !timingSafeEqual(supplied, expectedBytes)) {
      throw new Error(`Invalid OKX login signature on ${socket.url}`);
    }
    socket.loginVerified = true;
  }

  assertSubscribed(socket, message) {
    if (
      !(this.venue.validateSubscriptions ?? this.venue.validateLogin) ||
      !message.arg?.channel ||
      !Array.isArray(message.data)
    )
      return;
    const topics = [...socket.topics.values()].filter((topic) => topic.channel === message.arg.channel);
    // Some documented messages put instId only on each data row, not arg.
    const rows = message.data.length ? message.data : [null];
    for (const row of rows) {
      const object = row && !Array.isArray(row) && typeof row === 'object' ? row : {};
      const covered = topics.some((topic) =>
        Object.entries(topic).every(([key, value]) => {
          if (key === 'channel') return true;
          if (key === 'instType' && value === 'ANY') return true;
          const values = [message.arg[key], object[key]].filter((v) => v !== undefined && v !== '');
          if (!values.length && key === 'instType') {
            const id = object.instId ?? message.arg.instId;
            if (id?.endsWith('-SWAP')) values.push('SWAP');
          }
          return values.length > 0 && values.every((v) => v === value);
        }),
      );
      if (!covered) {
        throw new Error(
          `Unsubscribed ${message.arg.channel} frame on ${socket.url}: ${JSON.stringify(message.arg)}${object.instId ? ` instId=${object.instId}` : ''}`,
        );
      }
    }
  }

  assertWritesConsumed() {
    // The host may catch a transport exception and retain already-loaded state.
    // A failed verifier remains fatal at the final replay check.
    if (this.invalidSignatures) throw new Error(`${this.invalidSignatures} invalid replay signature(s)`);
    const missing = this.responses.unusedWrites();
    if (missing) throw new Error(`${missing} taped write(s) were not sent; command results: ${JSON.stringify(this.results)}`);
  }

  close(socket, code = 1000) {
    if (socket.closed) return;
    socket.closed = true;
    this.audit?.closed?.(socket.url, socket.generation);
    socket.onClose?.(code);
  }

  sockets(url, ordinal) {
    return [...(this.open.get(this.socketUrlKey(url)) ?? [])].filter((socket) =>
      !socket.closed && (ordinal === undefined || socket.ordinal === ordinal));
  }

  correlateSocketResponse(socket, text) {
    if (!this.venue.socketResponse) return text;
    let message;
    try {
      message = JSON.parse(text);
    } catch {
      return text;
    }
    const response = this.venue.socketResponse(message);
    if (!response) return text;
    if (this.auditEnabled && socket.responseIds.has(response.id)) throw new Error('duplicate websocket response');
    const scope = `${this.socketUrlKey(socket.url)} ${socket.ordinal}`;
    const recordedKey = `${scope} ${response.id}`;
    const recorded = this.recordedSocketRequests.get(recordedKey);
    if ((this.recordedSocketRequests.size || this.venue.requireSocketRequestRecords) && recorded === undefined)
      throw new Error('websocket response has no taped request');
    if (this.auditEnabled && recorded !== undefined && response.key !== undefined && recorded !== response.key)
      throw new Error('websocket response symbol differs from taped request');
    const key = recorded ?? response.key;
    const id = this.recordedSocketRequests.size
      ? socket.mappedRequests.get(response.id)
      : [...socket.requests].find(([, requestKey]) => requestKey === key)?.[0];
    if (id === undefined) throw new Error(`unsolicited websocket response ${key ?? response.id}`);
    if (this.auditEnabled && socket.requests.get(id) !== key) throw new Error('websocket response differs from pending request');
    socket.requests.delete(id);
    socket.mappedRequests.delete(response.id);
    if (this.auditEnabled) socket.responseIds.add(response.id);
    return JSON.stringify({ ...message, id });
  }

  async socketFor(url, text, ordinal, openingOnly = false) {
    if (typeof text === 'number') { ordinal = text; text = undefined; }
    const key = this.socketUrlKey(url);
    const stream = !openingOnly && text && this.venue.streamOf?.(text);
    const lifecycleKey = ordinal === undefined ? undefined : `${key} ${ordinal}`;
    const streams = !openingOnly && this.venue.subscriptionRouting && lifecycleKey
      ? this.lifecycleStreams.get(lifecycleKey) : undefined;
    const bound = !openingOnly && lifecycleKey && this.lifecycleSockets.get(lifecycleKey);
    if (this.venue.subscriptionRouting && !stream && bound && !bound.closed) return bound;
    if (!openingOnly && (stream || streams?.size) && this.venue.subscriptionRouting) ordinal = undefined;
    const subscribed = (socket, target) => [...socket.subscriptions].some((topic) => target === topic || target.startsWith(`${topic}.`));
    const active = () => [...(this.open.get(key) ?? [])].filter((socket) =>
      !socket.closed && (ordinal === undefined
        || (this.venue.socketOrdinalMatches?.(socket, ordinal) ?? socket.ordinal === ordinal))
      && (stream ? subscribed(socket, stream) : !streams?.size || [...streams].some((target) => subscribed(socket, target)))
      && (openingOnly || ordinal !== undefined || text === undefined || !this.venue.socketMatches
        || this.venue.socketMatches(socket, text)));
    if (!active().length) {
      let waiter;
      const pending = new Promise((resolve) => {
        waiter = () => (active().length ? (resolve(), true) : false);
        this.waiters.push(waiter);
        waiter();
      });
      try {
        await (this.replayClock?.active
          ? this.replayClock.wait(pending, `socket ${url} ${stream ?? 'open'}`, this.timeoutMs)
          : withTimeout(pending, `socket ${url} ${stream ?? 'open'}`, this.timeoutMs));
      } finally {
        this.waiters = this.waiters.filter((entry) => entry !== waiter);
      }
    }
    if (openingOnly || stream || ordinal !== undefined || this.venue.socketMatches || this.venue.rust !== 'hyperliquid') {
      const socket = active().at(-1);
      if (!openingOnly && this.venue.subscriptionRouting && lifecycleKey) this.lifecycleSockets.set(lifecycleKey, socket);
      return socket;
    }
    // Legacy tapes without a stream discriminator deliver to every listener,
    // including Hyperliquid's separate public/private listeners on one URL.
    return {
      onClose: (code) => {
        for (const socket of active()) this.close(socket, code);
      },
      onMessage: (text) => {
        for (const socket of active()) this.deliver(socket, text);
      },
    };
  }

  deliver(socket, text) {
    if (text === undefined && socket.text !== undefined) return this.deliverFrame(socket);
    if (typeof text === 'string' && this.venue.frameText) text = this.venue.frameText(text);
    if (this.auditEnabled && (this.venue.validateLogin || this.venue.validateSubscriptions) && typeof text === 'string' && text.trimStart().startsWith('{')) {
      const message = JSON.parse(text);
      if (
        this.venue.validateLogin &&
        message.event === 'login' &&
        String(message.code) === '0' &&
        !socket.loginVerified
      )
        throw new Error(`Taped login success preceded a valid login on ${socket.url}`);
      this.assertSubscribed(socket, message);
    }
    try { deliverSocketFrame(socket, text, this.audit, this.auditEnabled ? this.venue.validateSocketFrame : undefined); }
    catch(error) { this.invalidSignatures++; throw error; }
  }

  async deliverFrame(frame) {
    if (frame.type === 'close' && frame.socket === undefined && this.venue.socketMatches && this.sockets(frame.url).length > 1) {
      throw new Error('Ambiguous close on a shared public/private endpoint');
    }
    let text = frame.binary === undefined ? frame.text : Uint8Array.from(Buffer.from(frame.binary, 'base64'));
    const latest = await this.socketFor(frame.url, text, frame.socket);
    if (this.venue.bootstrapPrivateFromTape && frame.type === 'ws') {
      const message = JSON.parse(text);
      if (message.req_id === '@latest') {
        const deadline = performance.now() + this.timeoutMs;
        while (!latest.requests.has(message.op)) {
          if (performance.now() >= deadline)
            throw new ReplayTimeout(`replay timeout: ${message.op} request for acknowledgement`);
          await new Promise((resolve) => realTimeout(resolve, 5));
        }
        text = JSON.stringify({ ...message, req_id: latest.requests.get(message.op) });
      }
    }
    const sockets = this.bitget && frame.socket === undefined
      ? [...(this.open.get(this.socketUrlKey(frame.url)) ?? [])].filter((socket) => !socket.closed) : [latest];
    if (frame.type === 'close') {
      for (const socket of sockets) {
        if (socket.url) this.close(socket, frame.code ?? 1006);
        else socket.onClose(frame.code ?? 1006);
      }
      return;
    }
    if (frame.type === 'open') return;
    let arg;
    if (this.bitget) {
      try {
        arg = JSON.parse(frame.text)?.arg;
      } catch {
        /* Plain heartbeat. */
      }
    }
    const key = this.bitget && arg && this.topicKey(arg);
    const matching = key ? sockets.filter((socket) => socket.topics.has(key)) : sockets;
    if (key && !matching.length) throw new Error(`frame has no subscribed socket: ${frame.url} ${key}`);
    for (const socket of matching) {
      if (socket.url) this.deliver(socket, text);
      else socket.onMessage(text);
    }
  }

  /**
   * Deliver every frame in order, yielding to the event loop between frames.
   * Commands are awaited where they fall. A private tape is paced: each record
   * waits until its time after the account is ready.
   */
  async play(onTick, ready = async () => {}, runCommand = async () => {}, clock, begin = () => {}, checkpoint) {
    const first = this.frames.find((frame) => frame.url && frame.type !== 'wait' && frame.type !== 'http');
    const startupDeadline = performance.now() + this.timeoutMs;
    await within((async () => {
      // Fence only the first physical socket; a later command may create the
      // first frame's subscription. Bootstrap auth then selects its own socket.
      if (first) await this.socketFor(first.url, first.text, first.socket, true);
      for (const frame of this.frames.filter((frame) => this.bootstrapFrame(frame))) {
        await this.deliverFrame(frame);
        await new Promise(setImmediate);
      }
      await ready(!this.venue.bootstrapPrivateFromTape);
      // Native and TS outboxes batch sends independently of handler registration.
      // Begin the tape only after every seed-derived startup topic reached send.
      while (this.pendingStartupSubscriptions || (this.audit?.ready && !this.audit.ready())) {
        if (performance.now() >= startupDeadline) throw new ReplayTimeout('replay timeout: startup subscriptions');
        await new Promise((resolve) => realTimeout(resolve, 1));
      }
    })(), this.timeoutMs, 'Replay startup');
    begin();
    this.playing = true;
    clock?.start();
    this.replayClock = clock;
    const start = performance.now();
    let delivered = 0;
    const replayFrames = this.frames.filter((frame) => !this.bootstrapFrame(frame));
    const checkpoints = checkpointOrdinals(replayFrames);
    await checkpoint?.({ ordinal: 0, recordIndex: -1, t: 0 });
    const warm = Math.floor(replayFrames.length / 4);
    for (const frame of this.frames) {
      if (this.bootstrapFrame(frame)) continue;
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      if (this.isPrivate && !clock) {
        const wait = frame.t - (performance.now() - start);
        if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      }
      this.time = frame.t;
      await clock?.advance(frame.t);
      if (frame.type === 'command') {
        await (clock
          ? clock.wait(ready(true), 'account readiness', this.timeoutMs)
          : withTimeout(ready(true), 'account readiness', this.timeoutMs));
        this.commandIndex++;
        this.results.push(
          await (
            clock
              ? clock.wait(runCommand(frame), `command ${frame.op}`, this.timeoutMs)
              : withTimeout(runCommand(frame), `command ${frame.op}`, this.timeoutMs)
          ).then(
            (ids) => ({ ok: ids }),
            (error) => {
              if (error instanceof ReplayTimeout || String(error?.message).startsWith('replay timeout:')) throw error;
              return { error: String(error?.message ?? error) };
            },
          ),
        );
        this.settledCommands = this.commandIndex;
      } else if (frame.type === 'wait' || frame.type === 'http') {
        // Advance explicit horizons; real requests select HTTP responses.
      } else if (this.startupDelivered.has(frame) || this.earlyDelivered.has(frame)) {
        // Still advance tape time, frame count and tick callbacks below.
      } else {
        await this.deliverFrame(frame);
      }
      delivered++;
      onTick?.(frame.t);
      await new Promise(setImmediate);
      if (this.earlyFailure) throw this.earlyFailure;
      if (checkpoints.has(delivered))
        await checkpoint?.({ ordinal: delivered, recordIndex: this.recordIndices.get(frame), t: frame.t ?? 0 });
    }
    if (this.selectedPollingResponses) await this.finishPolling(clock);
    return delivered;
  }

  // Called outside the performance CPU and memory windows. Preserve request
  // multiplicity and mutation order; canonicalize generated transport IDs only
  // using the same input-side routing rules as the audited replay.
  transportReceipt() {
    const counts = (values) => {
      const rows = new Map();
      for (const value of values) rows.set(value, (rows.get(value) ?? 0) + 1);
      return [...rows].sort(([a], [b]) => a.localeCompare(b));
    };
    const subscriptions = [];
    const socketRequests = [];
    for (const { url, text, generation } of this.receiptSends) {
      for (const key of subscriptionKeys(url, text, this.callerIds, this.venue))
        subscriptions.push(`${generation} ${key}`);
      let message;
      try { message = JSON.parse(text); } catch { continue; }
      const request = this.venue.socketRequest?.(message);
      if (request) socketRequests.push(`${generation} ${canonicalUrl(url, this.callerIds, this.venue)} ${request.key}`);
    }
    const connections = [...this.open.values()].flatMap((sockets) => [...sockets].map((socket) => ({
      url: canonicalUrl(socket.url, this.callerIds, this.venue), ordinal: socket.ordinal, closed: socket.closed,
    }))).sort((a, b) => a.url.localeCompare(b.url) || a.ordinal - b.ordinal);
    return {
      requests: counts(this.receiptRequests.map(([key]) => key)),
      writes: this.receiptRequests.filter(([, write]) => write).map(([key]) => key),
      subscriptions: counts(subscriptions),
      socketRequests: counts(socketRequests),
      connections,
    };
  }

  unusedWrites() {
    return this.responses.unusedWrites();
  }

  get missingWrites() {
    return this.responses.unusedWrites();
  }
  get protocolSubscriptions() {
    return this.subscriptions;
  }
  isWrite(method, url) {
    return this.responses.isWrite(method, url);
  }
  invalidSignatures = 0;
  results = [];
  requests = new Set();
  subscriptions = new Set();
  writes = [];
  sentRequests = new Set();
  sent(url, text) {
    if (typeof url !== 'string') return this.send(url, text);
    if (!this.auditEnabled) return;
    if (!this.venue.requestBodies || text === 'ping' || text === 'pong') return;
    let message;
    try {
      message = JSON.parse(text);
    } catch {
      return;
    }
    const capture = (value) =>
      JSON.stringify(value, (_key, entry) =>
        entry && !Array.isArray(entry) && typeof entry === 'object'
          ? Object.fromEntries(Object.entries(entry).sort(([a], [b]) => a.localeCompare(b)))
          : entry,
      );
    if (message.op === 'subscribe' || message.op === 'unsubscribe') {
      for (const topic of message.args ?? []) this.sentRequests.add(capture({ url, op: message.op, topic }));
    } else if (message.op === 'login') {
      const args = message.args.map(({ timestamp, nonce, sign, ...credentials }) => credentials);
      this.sentRequests.add(capture({ url, op: 'login', args }));
    }
  }
}

// Compatibility factory uses the same reviewed transport as actual replay.
export const createTape = (records, venue, isPrivate, settings = {}) =>
  new Tape(
    records,
    venue,
    isPrivate,
    settings.socketTimeoutMs ?? 30000,
    settings.impl,
    settings.books ?? [],
    settings.credentials,
    settings,
  );
