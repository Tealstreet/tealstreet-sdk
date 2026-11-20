/** One captured action identity; callers validate its UUIDv7 timestamp at ingress. */
export type PlacementIntentIdentity = Readonly<{ id: string; createdAt: number }>;

type Entry<Result> = {
  createdAt: number;
  fingerprint: string;
  promise: Promise<Result>;
  settled: boolean;
};

type Refusal = 'intentConflict' | 'intentRetired' | 'intentCapacity' | 'invalidIntentId';

export class PlacementIntentError extends Error {
  readonly outcome: 'notSent' | 'unknown';

  constructor(readonly code: Refusal) {
    super({
      intentConflict: 'This intent conflicts with an earlier action; check the original order.',
      intentRetired: 'This intent was already retired; its prior outcome remains uncertain. It will not be sent again.',
      intentCapacity: 'Too many order intents are awaiting answers; this action was not sent.',
      invalidIntentId: 'The order intent identity is invalid; this action was not sent.',
    }[code]);
    this.name = 'PlacementIntentError';
    this.outcome = code === 'intentCapacity' || code === 'invalidIntentId' ? 'notSent' : 'unknown';
  }
}

/** One authority scope; never share a ledger between authenticated owners/accounts. */
export class PlacementIntentLedger<Result> {
  private readonly entries = new Map<string, Entry<Result>>();
  private retiredBefore = 0;

  constructor(readonly capacity = 1024) {
    if (!Number.isSafeInteger(capacity) || capacity < 1) throw new RangeError('Invalid intent capacity');
  }

  run(intent: PlacementIntentIdentity, fingerprint: string, send: () => Promise<Result>): Promise<Result> {
    if (!intent.id || !Number.isSafeInteger(intent.createdAt) || intent.createdAt < 0 || intent.createdAt > 0xffffffffffff)
      return Promise.reject(new PlacementIntentError('invalidIntentId'));
    const existing = this.entries.get(intent.id);
    if (existing) {
      if (existing.fingerprint !== fingerprint || existing.createdAt !== intent.createdAt)
        return Promise.reject(new PlacementIntentError('intentConflict'));
      return existing.promise;
    }
    if (intent.createdAt < this.retiredBefore)
      return Promise.reject(new PlacementIntentError('intentRetired'));
    if (this.entries.size >= this.capacity) {
      let oldest = Infinity;
      for (const entry of this.entries.values())
        if (entry.settled) oldest = Math.min(oldest, entry.createdAt);
      if (!Number.isFinite(oldest)) return Promise.reject(new PlacementIntentError('intentCapacity'));
      // Retire by timestamp, never by payload: forgotten IDs stay refused, including Unknowns.
      this.retiredBefore = Math.max(this.retiredBefore, oldest + 1);
      for (const [id, entry] of this.entries)
        if (entry.settled && entry.createdAt < this.retiredBefore) this.entries.delete(id);
      if (intent.createdAt < this.retiredBefore)
        return Promise.reject(new PlacementIntentError('intentRetired'));
    }
    const settled = () => {
      const entry = this.entries.get(intent.id);
      if (entry) entry.settled = true;
    };
    let start = () => {};
    const promise = new Promise<Result>((resolve, reject) => {
      start = () => {
        try {
          Promise.resolve(send()).then(
            (value) => { settled(); resolve(value); },
            (error: unknown) => { settled(); reject(error); },
          );
        } catch (error) {
          settled();
          reject(error);
        }
      };
    });
    // Reserve before invoking user callbacks, which can re-enter synchronously.
    this.entries.set(intent.id, { createdAt: intent.createdAt, fingerprint, promise, settled: false });
    start();
    return promise;
  }

  get size(): number { return this.entries.size; }
}
