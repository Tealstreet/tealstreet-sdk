import type { DelegatedAction, DelegatedActionResult } from './delegatedActions';

import { utf8ToBytes } from '@noble/hashes/utils.js';

import { delegatedActionFingerprint, isDelegatedAction, isDelegatedActionResult } from './delegatedActions';
import {
  createDelegatedIntentId,
  DELEGATED_ACK_MAX,
  DELEGATED_OUTCOME_MAX,
  DELEGATED_SUBMIT_MAX,
  delegatedIntentFingerprint,
  delegatedIntentTime,
} from './delegatedOrderFrames';

export type { DelegatedAction, DelegatedActionResult, DelegatedActionOutcome } from './delegatedActions';
export { createDelegatedIntentId } from './delegatedOrderFrames';

export interface OutboxScope {
  owner: string;
  accountName: string;
}
const delegatedNotSentBrand = Symbol('delegatedNotSent');
const delegatedReplayBrand = Symbol('delegatedReplay');
type ProvenNotSent = 'notSent' & { readonly [delegatedNotSentBrand]: true };
export type DelegatedReplayableIntent = DelegatedIntent & { readonly [delegatedReplayBrand]: true };
export type IntentState =
  'queued' | 'sending' | 'acknowledged' | 'applied' | 'placed' | 'refused' | 'unknown' | ProvenNotSent | 'canceled';

export type DelegatedNotSentEvidence =
  | { kind: 'noHandoff'; source: 'local'; attempted: false }
  | { kind: 'noHandoff'; source: 'adapter'; handedOff: false }
  | { kind: 'routerRefusal'; receipt: 'ack'; received: false }
  | {
      kind: 'routerRefusal';
      receipt: 'outcome';
      outcome: 'refused';
      errorOutcome: 'notSent';
      venueOrderId: null;
    };
type NoHandoffEvidence = Extract<DelegatedNotSentEvidence, { kind: 'noHandoff' }>;
interface EvidenceBinding extends OutboxScope {
  version: 1;
  intentId: string;
  seq: number;
  fingerprint: string;
}
export type StoredDelegatedNotSentEvidence = Readonly<DelegatedNotSentEvidence & EvidenceBinding>;
export type StoredDelegatedNoHandoffEvidence = Readonly<NoHandoffEvidence & EvidenceBinding>;

export interface DelegatedIntent extends OutboxScope {
  version: 1 | 2;
  action?: DelegatedAction;
  result?: DelegatedActionResult;
  fingerprint: string;
  intentId: string;
  callerSuppliedIntentId?: true;
  seq: number;
  params: Record<string, unknown>;
  clockEpoch: string;
  createdAt: number;
  state: IntentState;
  notSentEvidence?: StoredDelegatedNotSentEvidence;
  noHandoffEvidence?: StoredDelegatedNoHandoffEvidence;
  cancelPending: boolean;
  /** Unknown history expired after 24 hours; never evidence of non-forwarding. */
  retentionExpired?: boolean;
  venueOrderId?: string;
  error?: { message: string; outcome?: 'unknown' | ProvenNotSent };
}
export interface OutboxStorage {
  load(scope: Readonly<OutboxScope>): Promise<DelegatedIntent[] | OutboxSnapshot>;
  put(scope: Readonly<OutboxScope>, intent: DelegatedIntent): Promise<void>;
  /** Atomically delete the listed IDs and advance (never lower) the checkpoint. */
  compact(scope: Readonly<OutboxScope>, checkpoint: OutboxCompaction): Promise<void>;
}
export interface OutboxSnapshot {
  version: 1;
  sequence: number;
  intents: DelegatedIntent[];
}
export interface OutboxCompaction {
  version: 1;
  sequence: number;
  retiredIntentIds: string[];
}
export interface OutboxOptions extends OutboxScope {
  storage: OutboxStorage;
  clock: { epoch: string; now(): number; unix(): number };
  random(bytes: Uint8Array): void;
  schedule(callback: () => void, delayMs: number): () => void;
  changed?(intent: DelegatedIntent): void;
}
export interface OutboxConnection extends OutboxScope {
  venue: 'bybit';
  signingMode: 'delegated';
  ready: boolean;
  capabilities: {
    /** Advertised by the admitted box. */
    delegatedOrderFramesV1: boolean;
    /** Advertised by the router; absent on older pass-through routers. */
    delegatedOrderLedgerV1?: boolean;
    delegatedOrderActionsV1?: boolean;
    delegatedOrderActionLedgerV1?: boolean;
  };
  instanceId?: string;
  /** True only for physical handoff; false/throw means neither sent nor buffered for a later connection. */
  send(frame: string): boolean;
}

const MAX_AGE_MS = 10_000;
const MAX_PENDING = 1024;
const UNKNOWN_RETENTION_MS = 24 * 60 * 60 * 1000;
const COMPLETED_HISTORY = 32;
const STATES: readonly string[] = [
  'queued',
  'sending',
  'acknowledged',
  'placed',
  'applied',
  'refused',
  'unknown',
  'notSent',
  'canceled',
];
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

function readNotSentEvidence(value: unknown): DelegatedNotSentEvidence | undefined {
  if (!record(value)) return;
  if (value.kind === 'noHandoff') {
    if (value.source === 'local' && value.attempted === false)
      return { kind: 'noHandoff', source: 'local', attempted: false };
    if (value.source === 'adapter' && value.handedOff === false)
      return { kind: 'noHandoff', source: 'adapter', handedOff: false };
  }
  if (value.kind === 'routerRefusal') {
    if (value.receipt === 'ack' && value.received === false)
      return { kind: 'routerRefusal', receipt: 'ack', received: false };
    if (
      value.receipt === 'outcome' &&
      value.outcome === 'refused' &&
      value.errorOutcome === 'notSent' &&
      value.venueOrderId === null
    )
      return {
        kind: 'routerRefusal',
        receipt: 'outcome',
        outcome: 'refused',
        errorOutcome: 'notSent',
        venueOrderId: null,
      };
  }
}

function storedEvidence(row: DelegatedIntent, value: unknown): DelegatedNotSentEvidence | undefined {
  if (
    !record(value) ||
    value.version !== 1 ||
    value.owner !== row.owner ||
    value.accountName !== row.accountName ||
    value.intentId !== row.intentId ||
    value.seq !== row.seq ||
    value.fingerprint !== row.fingerprint
  )
    return;
  return readNotSentEvidence(value);
}

function storedNotSentEvidence(row: DelegatedIntent): DelegatedNotSentEvidence | undefined {
  return storedEvidence(row, row.notSentEvidence);
}

function storedNoHandoffEvidence(row: DelegatedIntent): NoHandoffEvidence | undefined {
  const proof = storedEvidence(row, row.noHandoffEvidence);
  return proof?.kind === 'noHandoff' ? proof : undefined;
}

function unprovenNotSent(row: DelegatedIntent): void {
  row.state = 'unknown';
  delete row.notSentEvidence;
  delete row.noHandoffEvidence;
  if (record(row.error) && row.error.outcome === 'notSent') row.error.outcome = 'unknown';
}

/** Only positive evidence for this intent can construct a notSent result. */
export function markDelegatedNotSent(row: DelegatedIntent, evidence: DelegatedNotSentEvidence): boolean {
  const proof = readNotSentEvidence(evidence);
  if (!proof || (proof.kind === 'noHandoff' && !['queued', 'canceled', 'notSent'].includes(row.state))) {
    unprovenNotSent(row);
    return false;
  }
  row.notSentEvidence = Object.freeze({
    ...proof,
    version: 1,
    owner: row.owner,
    accountName: row.accountName,
    intentId: row.intentId,
    seq: row.seq,
    fingerprint: row.fingerprint,
  });
  Object.defineProperty(row, delegatedNotSentBrand, {
    value: true,
    configurable: false,
    enumerable: false,
    writable: false,
  });
  row.state = 'notSent' as ProvenNotSent;
  delete row.noHandoffEvidence;
  delete row.retentionExpired;
  if (record(row.error)) row.error.outcome = row.state;
  return true;
}

/** Only whole-intent no-handoff evidence can authorize a queued submission. */
export function markDelegatedReplayable(
  row: DelegatedIntent,
  evidence: DelegatedNotSentEvidence,
): row is DelegatedReplayableIntent {
  const proof = readNotSentEvidence(evidence);
  if (!proof || proof.kind !== 'noHandoff' || row.state !== 'queued') {
    unprovenNotSent(row);
    return false;
  }
  delete row.notSentEvidence;
  delete row.noHandoffEvidence;
  // Keep the removable proof field after the non-serialized runtime brand.
  Object.defineProperty(row, delegatedReplayBrand, {
    value: true,
    configurable: false,
    enumerable: false,
    writable: false,
  });
  row.noHandoffEvidence = Object.freeze({
    ...proof,
    version: 1,
    owner: row.owner,
    accountName: row.accountName,
    intentId: row.intentId,
    seq: row.seq,
    fingerprint: row.fingerprint,
  });
  if (record(row.error) && row.error.outcome === 'notSent') row.error.outcome = 'unknown';
  return true;
}

export interface EnqueueOptions {
  intentId: string;
}

export class DelegatedIntentError extends Error {
  constructor(readonly code: 'invalidIntentId' | 'intentConflict' | 'intentNotDurable') {
    super(
      {
        invalidIntentId: 'Caller intent id must be a canonical UUIDv7',
        intentConflict: 'Intent id is already bound to different parameters',
        intentNotDurable: 'Original intent was not persisted; it cannot be submitted again',
      }[code],
    );
    this.name = 'DelegatedIntentError';
  }
}

export class DelegatedOutbox {
  private readonly scope: Readonly<OutboxScope>;
  private readonly intents = new Map<string, DelegatedIntent>();
  private readonly timers = new Map<string, () => void>();
  private readonly sent = new Map<string, Readonly<OutboxConnection>>();
  private readonly canceled = new Map<string, Readonly<OutboxConnection>>();
  private readonly durable = new Set<string>();
  // Identity durability survives a failed later state update. This is never
  // no-handoff evidence and cannot make an intent replayable.
  private readonly persistedIntentKeys = new Set<string>();
  // Only current, durably completed rows can release capacity or retire.
  private readonly completedIntents = new Map<string, DelegatedIntent>();
  private connection?: Readonly<OutboxConnection>;
  private active = true;
  private sequence = 0;
  private readonly outcomeSequences = new Map<string, number>();
  private work: Promise<unknown> = Promise.resolve();

  static async open(options: OutboxOptions): Promise<DelegatedOutbox> {
    const box = new DelegatedOutbox(options);
    const saved = await options.storage.load(box.scope);
    const rows = Array.isArray(saved) ? saved : saved.intents;
    if (!Array.isArray(saved)) {
      if (saved.version !== 1 || !Number.isSafeInteger(saved.sequence) || saved.sequence < 0)
        throw new Error('Invalid outbox checkpoint');
      box.sequence = saved.sequence;
    }
    if (!Array.isArray(rows) || rows.length > MAX_PENDING + COMPLETED_HISTORY)
      throw new Error('Outbox capacity exceeded');
    for (const saved of [...rows]) {
      const row = clone(saved);
      if (
        ![1, 2].includes(row.version) ||
        row.owner !== box.scope.owner ||
        row.accountName !== box.scope.accountName ||
        delegatedIntentTime(row.intentId) < 0 ||
        !Number.isSafeInteger(row.seq) ||
        row.seq < 1 ||
        !Number.isFinite(row.createdAt) ||
        row.createdAt < 0 ||
        !STATES.includes(row.state) ||
        !row.params ||
        typeof row.params !== 'object' ||
        Array.isArray(row.params) ||
        typeof row.cancelPending !== 'boolean' ||
        (row.callerSuppliedIntentId !== undefined && row.callerSuppliedIntentId !== true) ||
        (row.retentionExpired !== undefined && (row.retentionExpired !== true || row.state !== 'unknown')) ||
        box.intents.has(row.intentId)
      )
        throw new Error('Invalid persisted intent');
      if (
        (row.version === 2 &&
          (!isDelegatedAction(row.action) || Object.keys(row.params).length > 0 || row.state === 'placed')) ||
        (row.version === 1 && (row.action !== undefined || row.state === 'applied'))
      )
        throw new Error('Invalid persisted action');
      if (row.state === 'applied' && (!row.action || !isDelegatedActionResult(row.result, row.action.kind)))
        throw new Error('Invalid persisted action result');
      const fingerprint = row.action
        ? delegatedActionFingerprint(row.accountName, row.intentId, row.action, row.seq)
        : delegatedIntentFingerprint(row.accountName, row.intentId, row.params, row.seq);
      if (row.fingerprint !== fingerprint) throw new Error('Persisted fingerprint differs');
      box.sequence = Math.max(box.sequence, row.seq);
      // JSON cannot carry the private runtime brand. Validate and reissue it.
      if (row.state === 'notSent' || row.state === 'refused') {
        const proof = storedNotSentEvidence(row);
        if (proof) markDelegatedNotSent(row, proof);
        else unprovenNotSent(row);
      }
      // A persisted handoff can have crossed the physical socket boundary.
      if (row.state === 'sending' || row.state === 'acknowledged') {
        row.state = 'unknown';
        row.cancelPending = false;
      }
      // Legacy terminal labels cannot settle a still-pending router cancellation.
      if (row.cancelPending && ['notSent', 'canceled'].includes(row.state)) unprovenNotSent(row);
      if (row.state === 'queued') {
        const proof = storedNoHandoffEvidence(row);
        if (
          row.clockEpoch !== options.clock.epoch ||
          box.expired(row) ||
          !proof ||
          !markDelegatedReplayable(row, proof)
        )
          unprovenNotSent(row);
      }
      box.intents.set(row.intentId, row);
      await box.persist(row);
      box.arm(row);
    }
    if (box.pendingCount() > MAX_PENDING) throw new Error('Outbox capacity exceeded');
    await box.compact();
    return box;
  }

  private constructor(private readonly options: OutboxOptions) {
    if (!options.owner || !options.accountName || !options.clock.epoch) throw new Error('Outbox scope is required');
    if (typeof options.storage.compact !== 'function') throw new Error('Outbox storage requires atomic compaction');
    this.scope = Object.freeze({ owner: options.owner, accountName: options.accountName });
  }

  /** Connect only after this account's DELEGATED admission has completed. */
  connect(connection: OutboxConnection): Readonly<OutboxConnection> {
    if (
      !this.active ||
      connection.owner !== this.scope.owner ||
      connection.accountName !== this.scope.accountName ||
      connection.venue !== 'bybit' ||
      connection.signingMode !== 'delegated'
    )
      throw new Error('Outbox connection scope differs');
    if (this.connection) this.disconnect(this.connection);
    this.sent.clear();
    this.canceled.clear();
    this.outcomeSequences.clear();
    this.connection = Object.freeze({
      ...connection,
      capabilities: Object.freeze({ ...connection.capabilities }),
      instanceId: connection.instanceId || undefined,
    });
    return this.connection;
  }

  disconnect(connection: Readonly<OutboxConnection>): void {
    if (this.connection !== connection) return;
    this.connection = undefined;
    for (const row of this.intents.values()) {
      if (row.state !== 'sending' && row.state !== 'acknowledged') continue;
      row.state = 'unknown';
      this.stopTimer(row.intentId);
      this.changed(row);
      this.arm(row);
      void this.serial(async () => {
        if (!this.active) return;
        await this.persist(row);
        if (this.active) await this.compact();
      }).catch(() => {
        /* Write-ahead sending still restores as unknown. */
      });
    }
  }

  close(): void {
    this.active = false;
    this.connection = undefined;
    for (const row of this.intents.values()) {
      if (row.state !== 'sending' && row.state !== 'acknowledged') continue;
      row.state = 'unknown';
      this.changed(row);
    }
    for (const cancel of this.timers.values()) cancel();
    this.timers.clear();
    this.sent.clear();
    this.canceled.clear();
    this.outcomeSequences.clear();
  }

  entries(): DelegatedIntent[] {
    return [...this.intents.values()].map((row) => {
      this.normalizeNotSent(row);
      return clone(row);
    });
  }

  enqueueAction(action: DelegatedAction, options?: EnqueueOptions): Promise<string> {
    if (!isDelegatedAction(action)) return Promise.reject(new Error('Invalid delegated action'));
    try {
      return this.enqueueIntent({}, clone(action), options);
    } catch (error) {
      return Promise.reject(error);
    }
  }

  async enqueue(params: Record<string, unknown>, options?: EnqueueOptions): Promise<string> {
    return this.enqueueIntent(params, undefined, options);
  }

  private enqueueIntent(
    params: Record<string, unknown>,
    action?: DelegatedAction,
    options?: EnqueueOptions,
  ): Promise<string> {
    const createdAt = this.options.clock.now();
    if (!this.active) return Promise.reject(new Error('Outbox is closed or full'));
    if (!params || Array.isArray(params) || typeof params !== 'object' || params.clientOrderId !== undefined)
      return Promise.reject(new Error('Outbox owns the client order id'));
    if (options !== undefined) {
      try {
        if (typeof options?.intentId !== 'string') throw new Error('Invalid key');
        delegatedIntentTime(options.intentId);
      } catch {
        return Promise.reject(new DelegatedIntentError('invalidIntentId'));
      }
      const existing = this.intents.get(options.intentId);
      if (existing) {
        const fingerprint = action
          ? delegatedActionFingerprint(this.scope.accountName, existing.intentId, action, existing.seq)
          : delegatedIntentFingerprint(this.scope.accountName, existing.intentId, params, existing.seq);
        if (!!existing.action !== !!action || existing.fingerprint !== fingerprint)
          return Promise.reject(new DelegatedIntentError('intentConflict'));
        // Retry is an identity lookup, never evidence for changing handoff or
        // replay state. Serialize behind the original put and keep its brands.
        return this.serial(async () => {
          if (!this.active) throw new Error('Outbox is closed or full');
          if (!this.persistedIntentKeys.has(existing.intentId)) throw new DelegatedIntentError('intentNotDurable');
          existing.callerSuppliedIntentId = true;
          await this.persist(existing);
          return existing.intentId;
        });
      }
    }
    if (this.pendingCount() >= MAX_PENDING || this.intents.size >= MAX_PENDING + COMPLETED_HISTORY)
      return Promise.reject(new Error('Outbox is closed or full'));
    const intentId =
      options?.intentId ?? createDelegatedIntentId(Math.floor(this.options.clock.unix()), this.options.random);
    if (this.intents.has(intentId) || !Number.isSafeInteger(this.sequence + 1))
      return Promise.reject(new Error('Intent id or sequence exhausted'));
    const row: DelegatedIntent = {
      ...this.scope,
      version: action ? 2 : 1,
      ...(action ? { action } : {}),
      fingerprint: action
        ? delegatedActionFingerprint(this.scope.accountName, intentId, action, this.sequence + 1)
        : delegatedIntentFingerprint(this.scope.accountName, intentId, params, this.sequence + 1),
      intentId,
      ...(options ? { callerSuppliedIntentId: true as const } : {}),
      seq: ++this.sequence,
      params: clone(params),
      clockEpoch: this.options.clock.epoch,
      createdAt,
      state: 'queued',
      cancelPending: false,
    };
    if (!Number.isFinite(row.createdAt) || row.createdAt < 0)
      return Promise.reject(new Error('Invalid monotonic clock'));
    if (JSON.stringify(row.params).length > 60 * 1024)
      return Promise.reject(new Error('Order parameters are too large'));
    this.intents.set(intentId, row);
    markDelegatedReplayable(row, { kind: 'noHandoff', source: 'local', attempted: false });
    this.arm(row);
    this.changed(row);
    const connection = this.connection;
    return this.serial(async () => {
      if (!this.active) return intentId;
      try {
        await this.persist(row);
      } catch (error) {
        markDelegatedNotSent(row, { kind: 'noHandoff', source: 'local', attempted: false });
        this.stopTimer(row.intentId);
        this.changed(row);
        throw error;
      }
      if (connection && this.replayable(row)) await this.sendIntent(row, connection);
      await this.compact().catch(() => {
        // Keep retirement pending for a later flush without hiding the durable
        // intent ID after a successful physical handoff.
      });
      return intentId;
    });
  }

  flush(connection: Readonly<OutboxConnection>): Promise<void> {
    return this.serial(async () => {
      for (const row of [...this.intents.values()].sort((a, b) => a.seq - b.seq)) {
        if (!this.current(connection)) return;
        this.normalizeNotSent(row);
        if (!this.durable.has(row.intentId)) await this.persist(row);
        if (this.replayable(row)) await this.sendIntent(row, connection);
        if (row.cancelPending) await this.sendCancellation(row, connection);
      }
      await this.compact();
    });
  }

  receive(connection: Readonly<OutboxConnection>, frame: unknown): Promise<void> {
    return this.serial(async () => {
      if (!this.current(connection)) return;
      let parsed: unknown = frame;
      let wireSize: number | undefined;
      if (typeof parsed === 'string') {
        // Bound the actual received bytes before parsing can discard whitespace
        // or normalize escaped text. ACKs get their smaller bound below.
        if (parsed.length > DELEGATED_OUTCOME_MAX) return;
        wireSize = utf8ToBytes(parsed).length;
        if (wireSize > DELEGATED_OUTCOME_MAX) return;
        try {
          parsed = JSON.parse(parsed);
        } catch {
          return;
        }
      }
      if (
        !record(parsed) ||
        !['order.ack', 'order.outcome', 'order.actionAck', 'order.actionOutcome'].includes(
          typeof parsed.t === 'string' ? parsed.t : '',
        ) ||
        !record(parsed.d)
      )
        return;
      const d = parsed.d;
      if (d.accountName !== this.scope.accountName || d.instanceId !== connection.instanceId) return;
      const size = wireSize ?? utf8ToBytes(JSON.stringify(parsed)).length;
      if (
        size > (parsed.t === 'order.ack' || parsed.t === 'order.actionAck' ? DELEGATED_ACK_MAX : DELEGATED_OUTCOME_MAX)
      )
        return;
      const data = d;
      if (typeof data.intentId !== 'string') return;
      const row = this.intents.get(data.intentId);
      if (!row || data.seq !== row.seq) return;
      if (row.action) {
        if (!this.actionCurrent(connection) || !this.actionReceipt(row, parsed.t, data)) return;
      } else if (parsed.t === 'order.ack' && typeof data.received === 'boolean') {
        if (!['queued', 'sending', 'unknown', 'notSent', 'canceled'].includes(row.state)) return;
        if (row.retentionExpired && data.received) return;
        if (data.received) row.state = 'acknowledged';
        else markDelegatedNotSent(row, { kind: 'routerRefusal', receipt: 'ack', received: false });
        delete row.retentionExpired;
      } else if (
        parsed.t === 'order.outcome' &&
        (data.outcome === 'placed' || data.outcome === 'refused' || data.outcome === 'unknown')
      ) {
        if (data.clientOrderId !== null && data.clientOrderId !== row.intentId) return;
        // Router counters describe delivery on this captured socket, not the
        // business result. Another intent's live/replayed result is no fence.
        if (
          !Number.isSafeInteger(data.outcomeSeq) ||
          Number(data.outcomeSeq) <= (this.outcomeSequences.get(row.intentId) ?? 0)
        )
          return;
        if (data.outcome === 'placed' && (typeof data.venueOrderId !== 'string' || !data.venueOrderId)) return;
        if (
          record(data.error) &&
          data.error.outcome === 'notSent' &&
          (data.outcome !== 'refused' || data.venueOrderId !== null)
        )
          return;
        this.outcomeSequences.set(row.intentId, Number(data.outcomeSeq));
        // A refusal without admitted no-forwarding evidence is uncertainty,
        // never a terminal result. Only the constructor below can prove it.
        row.state = data.outcome === 'refused' ? 'unknown' : data.outcome;
        delete row.retentionExpired;
        if (record(data.error) && typeof data.error.message === 'string') {
          row.error = { message: data.error.message };
          if (data.error.outcome === 'notSent') {
            markDelegatedNotSent(row, {
              kind: 'routerRefusal',
              receipt: 'outcome',
              outcome: 'refused',
              errorOutcome: 'notSent',
              venueOrderId: null,
            });
          }
        }
        if (typeof data.venueOrderId === 'string') row.venueOrderId = data.venueOrderId;
      } else return;
      delete row.noHandoffEvidence;
      row.cancelPending = false;
      this.stopTimer(row.intentId);
      this.changed(row);
      this.arm(row);
      await this.persist(row);
      await this.compact();
    });
  }

  private actionReceipt(row: DelegatedIntent, kind: unknown, data: Record<string, unknown>): boolean {
    if (data.version !== 1 || !row.action) return false;
    if (kind === 'order.actionAck' && typeof data.received === 'boolean') {
      if (!['queued', 'sending', 'unknown', 'notSent', 'canceled'].includes(row.state)) return false;
      if (row.retentionExpired && data.received) return false;
      if (data.received) row.state = 'acknowledged';
      else markDelegatedNotSent(row, { kind: 'routerRefusal', receipt: 'ack', received: false });
      delete row.retentionExpired;
      return true;
    }
    if (
      kind !== 'order.actionOutcome' ||
      !Number.isSafeInteger(data.outcomeSeq) ||
      Number(data.outcomeSeq) <= (this.outcomeSequences.get(row.intentId) ?? 0)
    )
      return false;
    if (data.outcome === 'applied') {
      if (data.error !== null || !isDelegatedActionResult(data.result, row.action.kind)) return false;
      row.result = clone(data.result);
      row.state = 'applied';
      delete row.error;
    } else {
      if (
        data.result !== null ||
        !record(data.error) ||
        typeof data.error.message !== 'string' ||
        [...data.error.message].length > 512
      )
        return false;
      if (data.outcome === 'refused' && data.error.outcome === 'notSent') {
        row.error = { message: data.error.message };
        // result:null plus this admitted router refusal proves the whole action
        // was not handed off. Normalize it to the shared intent evidence shape.
        markDelegatedNotSent(row, {
          kind: 'routerRefusal',
          receipt: 'outcome',
          outcome: 'refused',
          errorOutcome: 'notSent',
          venueOrderId: null,
        });
      } else if (data.outcome === 'unknown' && data.error.outcome === 'unknown') {
        row.state = 'unknown';
        row.error = { message: data.error.message, outcome: 'unknown' };
      } else return false;
      delete row.result;
    }
    delete row.retentionExpired;
    this.outcomeSequences.set(row.intentId, Number(data.outcomeSeq));
    return true;
  }

  private actionCurrent(connection: Readonly<OutboxConnection>): boolean {
    return (
      this.current(connection) &&
      connection.capabilities.delegatedOrderActionsV1 === true &&
      connection.capabilities.delegatedOrderActionLedgerV1 === true
    );
  }

  cancel(intentId: string): Promise<void> {
    const row = this.intents.get(intentId);
    const handedOff =
      row?.state === 'sending' || row?.state === 'acknowledged' || (row?.state === 'unknown' && !row.retentionExpired);
    if (!this.active || !row || (row.state !== 'queued' && !handedOff)) return Promise.resolve();
    row.cancelPending = handedOff;
    row.state = row.cancelPending ? 'unknown' : 'canceled';
    if (row.action && row.cancelPending)
      row.error = { message: 'Action awaits router confirmation', outcome: 'unknown' };
    this.stopTimer(intentId);
    this.changed(row);
    this.arm(row);
    const connection = this.connection;
    return this.serial(async () => {
      if (!this.active) return;
      await this.persist(row);
      if (connection) await this.sendCancellation(row, connection);
      await this.compact();
    });
  }

  private current(connection: Readonly<OutboxConnection>): boolean {
    return (
      this.active &&
      this.connection === connection &&
      connection.ready &&
      connection.capabilities.delegatedOrderFramesV1 === true &&
      connection.capabilities.delegatedOrderLedgerV1 === true
    );
  }

  private expired(row: DelegatedIntent): boolean {
    if (row.clockEpoch !== this.options.clock.epoch) return true;
    const age = this.options.clock.now() - row.createdAt;
    return !Number.isFinite(age) || age < 0 || age >= MAX_AGE_MS;
  }

  private async sendIntent(row: DelegatedReplayableIntent, connection: Readonly<OutboxConnection>): Promise<void> {
    if (
      !(row.action ? this.actionCurrent(connection) : this.current(connection)) ||
      !this.replayable(row) ||
      this.sent.get(row.intentId) === connection
    )
      return;
    if (this.expired(row)) {
      this.expire(row);
      return;
    }
    // Write ahead of the physical handoff. On reload an ambiguous handoff is
    // unknown, even if saving a later acknowledgement or outcome failed.
    try {
      await this.options.storage.put(
        this.scope,
        clone({ ...row, state: 'sending' as const, notSentEvidence: undefined, noHandoffEvidence: undefined }),
      );
    } catch {
      // The queued intent is already durable. Preserve successful enqueue and
      // its identity while waiting for storage to prepare the handoff.
      return;
    }
    if (!(row.action ? this.actionCurrent(connection) : this.current(connection)) || !this.replayable(row)) return;
    if (this.expired(row)) {
      this.expire(row);
      return;
    }
    const frame = this.frame(connection, row.action ? 'order.action' : 'order.submit', {
      ...(row.action ? { version: 1, action: row.action } : { params: row.params }),
      intentId: row.intentId,
      fingerprint: row.fingerprint,
      seq: row.seq,
      ageMs: this.options.clock.now() - row.createdAt,
    });
    if (utf8ToBytes(frame).length > DELEGATED_SUBMIT_MAX) {
      const proof = storedNoHandoffEvidence(row);
      if (proof) markDelegatedNotSent(row, proof);
      else unprovenNotSent(row);
      this.stopTimer(row.intentId);
      this.changed(row);
      this.arm(row);
      await this.persist(row);
      return;
    }
    if (this.expired(row)) {
      this.expire(row);
      return;
    }
    // Set in-flight before invoking the synchronous adapter: a reentrant
    // deadline/disconnect must already treat this handoff as ambiguous.
    const noHandoff = storedNoHandoffEvidence(row);
    delete row.notSentEvidence;
    delete row.noHandoffEvidence;
    row.state = 'sending';
    this.sent.set(row.intentId, connection);
    let handedOff = false;
    try {
      handedOff = connection.send(frame);
    } catch {
      // The adapter contract guarantees a throw means no physical handoff.
    }
    if (!handedOff) {
      if (noHandoff) {
        row.state = row.cancelPending ? 'canceled' : 'queued';
        row.cancelPending = false;
        if (row.state === 'queued')
          markDelegatedReplayable(row, { kind: 'noHandoff', source: 'adapter', handedOff: false });
      } else row.state = 'unknown';
      this.stopTimer(row.intentId);
    }
    this.changed(row);
    this.arm(row);
    if (!handedOff && this.expired(row)) this.expire(row);
    // Keep the write-ahead sending record on storage failure after handoff.
    await this.persist(row).catch(() => undefined);
  }

  private async sendCancellation(row: DelegatedIntent, connection: Readonly<OutboxConnection>): Promise<void> {
    if (
      !(row.action ? this.actionCurrent(connection) : this.current(connection)) ||
      !row.cancelPending ||
      this.canceled.get(row.intentId) === connection
    )
      return;
    await this.persist(row);
    if (!(row.action ? this.actionCurrent(connection) : this.current(connection)) || !row.cancelPending) return;
    if (
      connection.send(
        this.frame(connection, row.action ? 'order.actionCancelQueued' : 'order.cancelQueued', {
          ...(row.action ? { version: 1 } : {}),
          intentId: row.intentId,
          seq: row.seq,
          fingerprint: row.fingerprint,
        }),
      )
    )
      this.canceled.set(row.intentId, connection);
  }

  private frame(connection: Readonly<OutboxConnection>, op: string, data: Record<string, unknown>): string {
    return JSON.stringify({
      t: op,
      d: { accountName: this.scope.accountName, instanceId: connection.instanceId, ...data },
    });
  }

  private arm(row: DelegatedIntent): void {
    if (!this.active) return;
    this.timers.get(row.intentId)?.();
    this.timers.delete(row.intentId);
    if (row.state === 'unknown') {
      if (row.retentionExpired) return;
      const delay = delegatedIntentTime(row.intentId) + UNKNOWN_RETENTION_MS - this.options.clock.unix();
      if (!Number.isFinite(delay)) return;
      if (delay <= 0) {
        this.expireUnknown(row);
        return;
      }
      this.timers.set(
        row.intentId,
        this.options.schedule(
          () => {
            if (!this.active || row.state !== 'unknown' || row.retentionExpired) return;
            this.timers.delete(row.intentId);
            this.arm(row);
          },
          Math.min(delay, UNKNOWN_RETENTION_MS),
        ),
      );
      return;
    }
    if (!['queued', 'sending', 'acknowledged'].includes(row.state)) return;
    this.timers.set(
      row.intentId,
      this.options.schedule(
        () => {
          if (!this.active || !['queued', 'sending', 'acknowledged'].includes(row.state)) return;
          this.timers.delete(row.intentId);
          if (this.expired(row)) this.expire(row);
          else this.arm(row);
        },
        Math.max(0, MAX_AGE_MS - (this.options.clock.now() - row.createdAt)),
      ),
    );
  }

  private expireUnknown(row: DelegatedIntent): void {
    if (!this.active || row.state !== 'unknown' || row.retentionExpired) return;
    row.retentionExpired = true;
    row.cancelPending = false;
    this.stopTimer(row.intentId);
    this.changed(row);
    this.saveExpired(row);
  }

  private expire(row: DelegatedIntent): void {
    if (!this.active || !['queued', 'sending', 'acknowledged'].includes(row.state)) return;
    const proof = this.replayable(row) ? storedNoHandoffEvidence(row) : undefined;
    if (proof) markDelegatedNotSent(row, proof);
    else unprovenNotSent(row);
    row.cancelPending = false;
    this.stopTimer(row.intentId);
    this.changed(row);
    this.arm(row);
    this.saveExpired(row);
  }

  private saveExpired(row: DelegatedIntent): void {
    // Capture row/scope now; deferred persistence can never change its owner.
    void this.serial(async () => {
      if (!this.active) return;
      await this.persist(row);
      if (this.active) await this.compact();
    }).catch(() => {
      /* The in-memory state and write-ahead record still prevent a replay. */
    });
  }

  private stopTimer(id: string): void {
    this.timers.get(id)?.();
    this.timers.delete(id);
    this.sent.delete(id);
    this.canceled.delete(id);
  }
  private normalizeNotSent(row: DelegatedIntent): void {
    if (row.state === 'queued') {
      if (!this.replayable(row)) unprovenNotSent(row);
    } else if (row.state === 'refused') {
      unprovenNotSent(row);
    } else if (row.state === 'notSent') {
      const marked = row as DelegatedIntent & { readonly [delegatedNotSentBrand]?: true };
      if (marked[delegatedNotSentBrand] === true && storedNotSentEvidence(row)) delete row.noHandoffEvidence;
      else unprovenNotSent(row);
    } else {
      delete row.notSentEvidence;
      delete row.noHandoffEvidence;
      if (record(row.error) && row.error.outcome === 'notSent') row.error.outcome = 'unknown';
    }
    this.refreshCompleted(row);
  }
  private refreshCompleted(row: DelegatedIntent): void {
    if (this.intents.get(row.intentId) !== row) return;
    if (this.durable.has(row.intentId) && this.completed(row)) this.completedIntents.set(row.intentId, row);
    else this.completedIntents.delete(row.intentId);
  }
  private replayable(row: DelegatedIntent): row is DelegatedReplayableIntent {
    const marked = row as DelegatedIntent & { readonly [delegatedReplayBrand]?: true };
    return row.state === 'queued' && marked[delegatedReplayBrand] === true && !!storedNoHandoffEvidence(row);
  }
  private changed(row: DelegatedIntent): void {
    this.normalizeNotSent(row);
    this.durable.delete(row.intentId);
    this.completedIntents.delete(row.intentId);
    if (this.active) this.options.changed?.(clone(row));
  }
  private async persist(row: DelegatedIntent): Promise<void> {
    if (this.intents.get(row.intentId) !== row) return;
    this.normalizeNotSent(row);
    const saved = clone(row);
    await this.options.storage.put(this.scope, saved);
    this.persistedIntentKeys.add(row.intentId);
    // A synchronous deadline/cancellation can change memory during this await.
    if (JSON.stringify(saved) === JSON.stringify(row)) this.durable.add(row.intentId);
    else this.durable.delete(row.intentId);
    this.refreshCompleted(row);
  }
  private completed(row: DelegatedIntent): boolean {
    return (
      !row.cancelPending &&
      (row.retentionExpired === true ||
        row.state === 'applied' ||
        row.state === 'placed' ||
        row.state === 'refused' ||
        row.state === 'notSent' ||
        row.state === 'canceled')
    );
  }
  private pendingCount(): number {
    return this.intents.size - this.completedIntents.size;
  }
  private async compact(): Promise<void> {
    if (this.completedIntents.size <= COMPLETED_HISTORY) return;
    // Caller keys remain reserved across completed-history retirement.
    const completed = [...this.completedIntents.values()].filter((row) => !row.callerSuppliedIntentId);
    if (completed.length <= COMPLETED_HISTORY) return;
    let insertionOrder: Map<string, number> | undefined;
    completed.sort((a, b) => {
      if (a.seq !== b.seq) return a.seq - b.seq;
      // Legacy rows may share a sequence: retain the original stable tie order.
      insertionOrder ??= new Map([...this.intents.keys()].map((id, index) => [id, index] as const));
      return insertionOrder.get(a.intentId)! - insertionOrder.get(b.intentId)!;
    });
    const retired = completed.slice(0, completed.length - COMPLETED_HISTORY);
    await this.options.storage.compact(this.scope, {
      version: 1,
      sequence: this.sequence,
      retiredIntentIds: retired.map((row) => row.intentId),
    });
    for (const row of retired) {
      this.stopTimer(row.intentId);
      this.intents.delete(row.intentId);
      this.durable.delete(row.intentId);
      this.persistedIntentKeys.delete(row.intentId);
      this.completedIntents.delete(row.intentId);
      this.outcomeSequences.delete(row.intentId);
    }
  }
  private serial<T>(operation: () => Promise<T>): Promise<T> {
    const next = this.work.then(operation);
    this.work = next.catch(() => undefined);
    return next;
  }
}

function record(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value);
}
