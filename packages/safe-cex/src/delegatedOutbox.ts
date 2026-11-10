import { utf8ToBytes } from '@noble/hashes/utils.js';

import {
  createDelegatedIntentId,
  DELEGATED_ACK_MAX,
  DELEGATED_OUTCOME_MAX,
  DELEGATED_SUBMIT_MAX,
  delegatedIntentFingerprint,
  delegatedIntentTime,
} from './delegatedOrderFrames';

export { createDelegatedIntentId } from './delegatedOrderFrames';

export interface OutboxScope {
  owner: string;
  accountName: string;
}
export type IntentState =
  'queued' | 'sending' | 'acknowledged' | 'placed' | 'refused' | 'unknown' | 'notSent' | 'canceled';
export interface DelegatedIntent extends OutboxScope {
  version: 1;
  fingerprint: string;
  intentId: string;
  /** Caller keys remain reserved after their completed history ages. */
  callerSuppliedIntentId?: true;
  seq: number;
  params: Record<string, unknown>;
  clockEpoch: string;
  createdAt: number;
  state: IntentState;
  cancelPending: boolean;
  venueOrderId?: string;
  error?: { message: string; outcome?: string };
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
  };
  instanceId?: string;
  send(frame: string): boolean;
}

const MAX_AGE_MS = 10_000;
const MAX_PENDING = 1024;
const COMPLETED_HISTORY = 32;
const STATES: IntentState[] = [
  'queued',
  'sending',
  'acknowledged',
  'placed',
  'refused',
  'unknown',
  'notSent',
  'canceled',
];
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

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
  private readonly submitted = new Set<string>();
  private readonly durable = new Set<string>();
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
        row.version !== 1 ||
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
        box.intents.has(row.intentId)
      )
        throw new Error('Invalid persisted intent');
      if (row.fingerprint !== delegatedIntentFingerprint(row.accountName, row.intentId, row.params, row.seq))
        throw new Error('Persisted fingerprint differs');
      box.sequence = Math.max(box.sequence, row.seq);
      // A persisted handoff can have crossed the physical socket boundary.
      if (row.state === 'sending') {
        row.state = 'unknown';
        row.cancelPending = false;
      }
      if (row.state === 'queued' && (row.clockEpoch !== options.clock.epoch || box.expired(row))) row.state = 'notSent';
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
    if (this.connection === connection) this.connection = undefined;
  }

  close(): void {
    this.active = false;
    this.connection = undefined;
    for (const cancel of this.timers.values()) cancel();
    this.timers.clear();
    this.sent.clear();
    this.canceled.clear();
    this.submitted.clear();
    this.outcomeSequences.clear();
  }

  entries(): DelegatedIntent[] {
    return [...this.intents.values()].map((row) => clone(row));
  }

  async enqueue(params: Record<string, unknown>, options?: EnqueueOptions): Promise<string> {
    const createdAt = this.options.clock.now();
    if (!this.active) throw new Error('Outbox is closed or full');
    if (!params || Array.isArray(params) || typeof params !== 'object' || params.clientOrderId !== undefined)
      throw new Error('Outbox owns the client order id');
    if (options !== undefined) {
      try {
        if (typeof options?.intentId !== 'string') throw new Error('Invalid key');
        delegatedIntentTime(options.intentId);
      } catch {
        throw new DelegatedIntentError('invalidIntentId');
      }
      const existing = this.intents.get(options.intentId);
      if (existing) {
        if (
          existing.fingerprint !==
          delegatedIntentFingerprint(this.scope.accountName, existing.intentId, params, existing.seq)
        )
          throw new DelegatedIntentError('intentConflict');
        // Serialize behind the original put: a retry must not report durability
        // before that write commits, or turn its failure into another submission.
        return this.serial(async () => {
          if (!this.active) throw new Error('Outbox is closed or full');
          if (!this.durable.has(existing.intentId)) throw new DelegatedIntentError('intentNotDurable');
          // Persist the reservation even when a preceding promotion put failed.
          // An in-memory flag alone must not acknowledge a durable caller key.
          existing.callerSuppliedIntentId = true;
          await this.persist(existing);
          return existing.intentId;
        });
      }
    }
    if (this.pendingCount() >= MAX_PENDING || this.intents.size >= MAX_PENDING + COMPLETED_HISTORY)
      throw new Error('Outbox is closed or full');
    const intentId =
      options?.intentId ?? createDelegatedIntentId(Math.floor(this.options.clock.unix()), this.options.random);
    if (this.intents.has(intentId) || !Number.isSafeInteger(this.sequence + 1))
      return Promise.reject(new Error('Intent id or sequence exhausted'));
    const row: DelegatedIntent = {
      ...this.scope,
      version: 1,
      fingerprint: delegatedIntentFingerprint(this.scope.accountName, intentId, params, this.sequence + 1),
      intentId,
      ...(options ? { callerSuppliedIntentId: true as const } : {}),
      seq: this.sequence + 1,
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
    this.sequence = row.seq;
    this.intents.set(intentId, row);
    this.arm(row);
    this.changed(row);
    const connection = this.connection;
    return this.serial(async () => {
      if (!this.active) return intentId;
      try {
        await this.persist(row);
      } catch (error) {
        row.state = 'notSent';
        this.stopTimer(row.intentId);
        this.changed(row);
        throw error;
      }
      if (connection) await this.sendIntent(row, connection);
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
        if (!this.durable.has(row.intentId) && this.completed(row)) await this.persist(row);
        if (row.state === 'queued') await this.sendIntent(row, connection);
        if (row.cancelPending) await this.sendCancellation(row, connection);
      }
      await this.compact();
    });
  }

  receive(connection: Readonly<OutboxConnection>, frame: unknown): Promise<void> {
    return this.serial(async () => {
      if (!this.current(connection)) return;
      let parsed: unknown = frame;
      if (typeof parsed === 'string') {
        try {
          parsed = JSON.parse(parsed);
        } catch {
          return;
        }
      }
      if (!record(parsed) || (parsed.t !== 'order.ack' && parsed.t !== 'order.outcome') || !record(parsed.d)) return;
      const d = parsed.d;
      if (d.accountName !== this.scope.accountName || d.instanceId !== connection.instanceId) return;
      const size = utf8ToBytes(JSON.stringify(parsed)).length;
      if (size > (parsed.t === 'order.ack' ? DELEGATED_ACK_MAX : DELEGATED_OUTCOME_MAX)) return;
      const data = d;
      if (typeof data.intentId !== 'string') return;
      const row = this.intents.get(data.intentId);
      if (!row || data.seq !== row.seq) return;
      if (parsed.t === 'order.ack' && typeof data.received === 'boolean') {
        if (!['queued', 'notSent', 'canceled'].includes(row.state)) return;
        row.state = data.received ? 'acknowledged' : 'notSent';
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
        this.outcomeSequences.set(row.intentId, Number(data.outcomeSeq));
        row.state = data.outcome as IntentState;
        if (record(data.error) && typeof data.error.message === 'string') {
          row.error = { message: data.error.message };
          if (data.error.outcome === 'notSent') {
            row.state = 'notSent';
            row.error.outcome = 'notSent';
          }
        }
        if (typeof data.venueOrderId === 'string') row.venueOrderId = data.venueOrderId;
      } else return;
      row.cancelPending = false;
      this.stopTimer(row.intentId);
      this.changed(row);
      await this.persist(row);
      await this.compact();
    });
  }

  cancel(intentId: string): Promise<void> {
    const row = this.intents.get(intentId);
    if (!this.active || !row || row.state !== 'queued') return Promise.resolve();
    row.state = 'canceled';
    row.cancelPending = this.submitted.has(intentId);
    this.stopTimer(intentId);
    this.changed(row);
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

  private async sendIntent(row: DelegatedIntent, connection: Readonly<OutboxConnection>): Promise<void> {
    if (!this.current(connection) || row.state !== 'queued' || this.sent.get(row.intentId) === connection) return;
    if (this.expired(row)) {
      this.expire(row);
      return;
    }
    // Write ahead of the physical handoff. On reload an ambiguous handoff is
    // unknown, even if saving a later acknowledgement or outcome failed.
    try {
      await this.options.storage.put(this.scope, clone({ ...row, state: 'sending' as const }));
    } catch {
      // The queued intent is already durable. Preserve successful enqueue and
      // its identity while waiting for storage to prepare the handoff.
      return;
    }
    if (!this.current(connection) || row.state !== 'queued') return;
    if (this.expired(row)) {
      this.expire(row);
      return;
    }
    const previouslySubmitted = this.submitted.has(row.intentId);
    this.sent.set(row.intentId, connection);
    try {
      const frame = this.frame(connection, 'order.submit', {
        intentId: row.intentId,
        fingerprint: row.fingerprint,
        params: row.params,
        seq: row.seq,
        ageMs: this.options.clock.now() - row.createdAt,
      });
      if (utf8ToBytes(frame).length > DELEGATED_SUBMIT_MAX) {
        row.state = 'notSent';
        this.stopTimer(row.intentId);
        this.changed(row);
        await this.persist(row);
        return;
      }
      if (this.expired(row)) {
        this.expire(row);
        return;
      }
      this.submitted.add(row.intentId);
      const sent = connection.send(frame);
      if (!sent) {
        this.sent.delete(row.intentId);
        if (!previouslySubmitted) {
          this.submitted.delete(row.intentId);
          row.cancelPending = false;
        }
        await this.persist(row);
      }
    } catch {
      row.state = 'unknown';
      row.cancelPending = false;
      this.stopTimer(row.intentId);
      this.changed(row);
      await this.persist(row);
    }
  }

  private async sendCancellation(row: DelegatedIntent, connection: Readonly<OutboxConnection>): Promise<void> {
    if (!this.current(connection) || !row.cancelPending || this.canceled.get(row.intentId) === connection) return;
    await this.persist(row);
    if (!this.current(connection) || !row.cancelPending) return;
    if (
      connection.send(
        this.frame(connection, 'order.cancelQueued', {
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
    if (row.state !== 'queued') return;
    this.timers.set(
      row.intentId,
      this.options.schedule(
        () => {
          if (!this.active || row.state !== 'queued') return;
          this.timers.delete(row.intentId);
          if (this.expired(row)) this.expire(row);
          else this.arm(row);
        },
        Math.max(0, MAX_AGE_MS - (this.options.clock.now() - row.createdAt)),
      ),
    );
  }

  private expire(row: DelegatedIntent): void {
    if (!this.active || row.state !== 'queued') return;
    row.state = 'notSent';
    row.cancelPending = this.submitted.has(row.intentId);
    this.stopTimer(row.intentId);
    this.changed(row);
    const connection = this.connection;
    // The state changes synchronously at the deadline; storage latency cannot
    // defer the last handoff check or attribute work to another connection.
    void this.serial(async () => {
      if (!this.active) return;
      await this.persist(row);
      if (connection) await this.sendCancellation(row, connection);
      await this.compact();
    }).catch(() => {
      /* The in-memory terminal state still prevents a send. */
    });
  }

  private stopTimer(id: string): void {
    this.timers.get(id)?.();
    this.timers.delete(id);
    this.sent.delete(id);
    this.canceled.delete(id);
    this.submitted.delete(id);
  }
  private changed(row: DelegatedIntent): void {
    if (this.active) this.options.changed?.(clone(row));
  }
  private async persist(row: DelegatedIntent): Promise<void> {
    if (this.intents.get(row.intentId) !== row) return;
    await this.options.storage.put(this.scope, clone(row));
    this.durable.add(row.intentId);
  }
  private completed(row: DelegatedIntent): boolean {
    return !row.cancelPending && ['placed', 'refused', 'notSent', 'canceled'].includes(row.state);
  }
  private pendingCount(): number {
    return [...this.intents.values()].filter((row) => !this.durable.has(row.intentId) || !this.completed(row)).length;
  }
  private async compact(): Promise<void> {
    const completed = [...this.intents.values()]
      .filter((row) => this.durable.has(row.intentId) && this.completed(row) && !row.callerSuppliedIntentId)
      .sort((a, b) => a.seq - b.seq);
    const retired = completed.slice(0, Math.max(0, completed.length - COMPLETED_HISTORY));
    if (!retired.length) return;
    await this.options.storage.compact(this.scope, {
      version: 1,
      sequence: this.sequence,
      retiredIntentIds: retired.map((row) => row.intentId),
    });
    for (const row of retired) {
      this.stopTimer(row.intentId);
      this.intents.delete(row.intentId);
      this.durable.delete(row.intentId);
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
