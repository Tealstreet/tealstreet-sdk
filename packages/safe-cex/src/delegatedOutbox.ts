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
  load(scope: Readonly<OutboxScope>): Promise<DelegatedIntent[]>;
  put(scope: Readonly<OutboxScope>, intent: DelegatedIntent): Promise<void>;
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
  capabilities: { delegatedOrderFramesV1: boolean };
  instanceId?: string;
  send(frame: string): boolean;
}

const MAX_AGE_MS = 10_000;
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

export class DelegatedOutbox {
  private readonly scope: Readonly<OutboxScope>;
  private readonly intents = new Map<string, DelegatedIntent>();
  private readonly timers = new Map<string, () => void>();
  private readonly sent = new Map<string, Readonly<OutboxConnection>>();
  private readonly canceled = new Map<string, Readonly<OutboxConnection>>();
  private readonly submitted = new Set<string>();
  private connection?: Readonly<OutboxConnection>;
  private active = true;
  private sequence = 0;
  private outcomeSequence = 0;
  private work: Promise<unknown> = Promise.resolve();

  static async open(options: OutboxOptions): Promise<DelegatedOutbox> {
    const box = new DelegatedOutbox(options);
    const rows = await options.storage.load(box.scope);
    if (rows.length > 1024) throw new Error('Outbox capacity exceeded');
    for (const saved of rows) {
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
    return box;
  }

  private constructor(private readonly options: OutboxOptions) {
    if (!options.owner || !options.accountName || !options.clock.epoch) throw new Error('Outbox scope is required');
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
    this.outcomeSequence = 0;
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
  }

  entries(): DelegatedIntent[] {
    return [...this.intents.values()].map((row) => clone(row));
  }

  enqueue(params: Record<string, unknown>): Promise<string> {
    const createdAt = this.options.clock.now();
    if (!this.active || this.intents.size >= 1024) return Promise.reject(new Error('Outbox is closed or full'));
    if (!params || Array.isArray(params) || typeof params !== 'object' || params.clientOrderId !== undefined)
      return Promise.reject(new Error('Outbox owns the client order id'));
    const intentId = createDelegatedIntentId(Math.floor(this.options.clock.unix()), this.options.random);
    if (this.intents.has(intentId) || !Number.isSafeInteger(this.sequence + 1))
      return Promise.reject(new Error('Intent id or sequence exhausted'));
    const row: DelegatedIntent = {
      ...this.scope,
      version: 1,
      fingerprint: delegatedIntentFingerprint(this.scope.accountName, intentId, params, this.sequence + 1),
      intentId,
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
    this.arm(row);
    this.changed(row);
    const connection = this.connection;
    return this.serial(async () => {
      if (!this.active) return intentId;
      try {
        await this.persist(row);
      } catch (error) {
        row.state = 'notSent';
        this.changed(row);
        throw error;
      }
      if (connection) await this.sendIntent(row, connection);
      return intentId;
    });
  }

  flush(connection: Readonly<OutboxConnection>): Promise<void> {
    return this.serial(async () => {
      for (const row of [...this.intents.values()].sort((a, b) => a.seq - b.seq)) {
        if (!this.current(connection)) return;
        if (row.state === 'queued') await this.sendIntent(row, connection);
        if (row.cancelPending) await this.sendCancellation(row, connection);
      }
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
      if (!record(parsed) || !['order.ack', 'order.outcome'].includes(String(parsed.t)) || !record(parsed.d)) return;
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
      } else if (parsed.t === 'order.outcome' && ['placed', 'refused', 'unknown'].includes(String(data.outcome))) {
        if (data.clientOrderId !== null && data.clientOrderId !== row.intentId) return;
        if (!Number.isSafeInteger(data.outcomeSeq) || Number(data.outcomeSeq) <= this.outcomeSequence) return;
        if (data.outcome === 'placed' && (typeof data.venueOrderId !== 'string' || !data.venueOrderId)) return;
        this.outcomeSequence = Number(data.outcomeSeq);
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
    });
  }

  private current(connection: Readonly<OutboxConnection>): boolean {
    return (
      this.active &&
      this.connection === connection &&
      connection.ready &&
      connection.capabilities.delegatedOrderFramesV1 === true
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
    await this.options.storage.put(this.scope, clone({ ...row, state: 'sending' as const }));
    if (!this.current(connection) || row.state !== 'queued') return;
    if (this.expired(row)) {
      this.expire(row);
      return;
    }
    this.submitted.add(row.intentId);
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
      const sent = connection.send(frame);
      if (!sent) {
        this.sent.delete(row.intentId);
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
  private persist(row: DelegatedIntent): Promise<void> {
    return this.options.storage.put(this.scope, clone(row));
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
