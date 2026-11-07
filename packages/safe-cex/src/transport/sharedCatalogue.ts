import type { HttpPacket, HttpResponse } from '@tealstreet/safe-cex-wasm/host';

import { HttpPreparationError } from '@tealstreet/safe-cex-wasm/host';

const POLL_MS = 100;
const STORAGE_MS = 250;
type Entry = {
  owner: string;
  at: number;
  bytes?: Uint8Array;
  leaseUntil?: number;
  cooldownUntil?: number;
  failures?: number;
};
type Decision<T> = { value: T; write?: Entry | null };
export interface CatalogueRoute {
  network: 'mainnet' | 'testnet';
  route: string;
}
export interface CataloguePolicy {
  database: string;
  ttlMs: number;
  leaseMs: number;
  maxBytes: number;
  maxEntries: number;
  strict?: boolean;
  keyFor(request: HttpPacket & { intent?: string }, route?: CatalogueRoute): string | undefined;
  valid(key: string, bytes: Uint8Array): boolean;
  failureDelay?(failures: number, headers: string, now: number): number;
}
export interface CatalogueRead {
  cached?: HttpResponse;
  cachedAt?: number;
  expiresAt?: number;
  leaseExpiresAt?: number;
  leaseStartedAt?: number;
  complete(response: HttpResponse): Promise<HttpResponse>;
  failed?(headers?: string): Promise<void>;
  timedOut?(): void;
  cancel(): void;
}

const delay = (ms: number, signal: AbortSignal) =>
  new Promise<void>((resolve, reject) => {
    signal.throwIfAborted();
    const abort = () => {
      clearTimeout(timer);
      reject(signal.reason);
    };
    const timer = setTimeout(() => {
      signal.removeEventListener('abort', abort);
      resolve();
    }, ms);
    signal.addEventListener('abort', abort, { once: true });
  });

/** Shares only anonymous catalogues; every account read and write stays with its engine. */
export class SharedCatalogue {
  private database?: Promise<IDBDatabase>;
  private retryAt = 0;
  private coordinated = false;
  constructor(
    private readonly policy: CataloguePolicy,
    private readonly factory?: IDBFactory,
  ) {}

  private live(entry?: Entry): boolean {
    if (!entry || !Number.isFinite(entry.at)) return false;
    if (Date.now() < entry.at) return this.policy.strict === true && !entry.bytes;
    const until = entry.bytes
      ? entry.at + this.policy.ttlMs
      : (entry.cooldownUntil ?? entry.leaseUntil ?? entry.at + this.policy.leaseMs);
    // Retain an expired lease through its failure window while its owner settles the deadline.
    const grace =
      this.policy.strict && !entry.bytes && entry.cooldownUntil === undefined
        ? (this.policy.failureDelay?.(Math.min(6, (entry.failures ?? 0) + 1), '', until) ?? 0)
        : 0;
    return (
      Number.isFinite(until) && until <= entry.at + Math.max(this.policy.leaseMs, 300_000) && Date.now() < until + grace
    );
  }

  private open(): Promise<IDBDatabase> {
    if (this.database) return this.database;
    const attempt = new Promise<IDBDatabase>((resolve, reject) => {
      let settled = false;
      const fail = () => {
        if (!settled) {
          settled = true;
          clearTimeout(timer);
          reject(new Error('Catalogue cache unavailable'));
        }
      };
      const timer = setTimeout(fail, STORAGE_MS);
      try {
        const request = (this.factory ?? indexedDB).open(this.policy.database, 1);
        request.onupgradeneeded = () => {
          try {
            if (settled) request.transaction?.abort();
            else request.result.createObjectStore('catalogues');
          } catch {
            fail();
          }
        };
        request.onblocked = request.onerror = fail;
        request.onsuccess = () => {
          if (settled) {
            request.result.close();
            return;
          }
          settled = true;
          clearTimeout(timer);
          request.result.onversionchange = () => {
            request.result.close();
            this.database = undefined;
          };
          resolve(request.result);
        };
      } catch {
        fail();
      }
    });
    this.database = attempt;
    void attempt.catch(() => {
      if (this.database === attempt) this.database = undefined;
    });
    return attempt;
  }

  private async transaction<T>(
    work: (store: IDBObjectStore, done: (value: T) => void, fail: () => void) => void,
  ): Promise<T> {
    // A refused retry is not another storage failure and must not renew the pause.
    if (this.retryAt > Date.now()) throw new Error('Catalogue cache unavailable');
    try {
      const db = await this.open();
      return await new Promise<T>((resolve, reject) => {
        const transaction = db.transaction('catalogues', 'readwrite');
        let settled = false;
        let value: T;
        const fail = () => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          try {
            transaction.abort();
          } catch {
            /* A completed transaction is already released. */
          }
          reject(new Error('Catalogue cache unavailable'));
        };
        const timer = setTimeout(fail, STORAGE_MS);
        transaction.onerror = transaction.onabort = fail;
        transaction.oncomplete = () => {
          if (!settled) {
            settled = true;
            clearTimeout(timer);
            this.coordinated = true;
            resolve(value);
          }
        };
        try {
          work(
            transaction.objectStore('catalogues'),
            (next) => {
              value = next;
            },
            fail,
          );
        } catch {
          fail();
        }
      });
    } catch (error) {
      this.retryAt = Date.now() + 30_000;
      void this.database?.then((db) => db.close()).catch(() => undefined);
      this.database = undefined;
      throw error;
    }
  }

  private edit<T>(key: string, change: (entry?: Entry) => Decision<T>): Promise<T> {
    return this.transaction<T>((store, done, fail) => {
      const request = store.get(key);
      request.onsuccess = () => {
        try {
          const decision = change(request.result);
          done(decision.value);
          if (decision.write === null) store.delete(key);
          else if (decision.write) {
            if (request.result) store.put(decision.write, key);
            else {
              let count = 0;
              const cursor = store.openCursor();
              cursor.onsuccess = () => {
                try {
                  const row = cursor.result;
                  if (row) {
                    if (!this.live(row.value)) row.delete();
                    else count += 1;
                    row.continue();
                  } else if (count >= this.policy.maxEntries) fail();
                  else store.put(decision.write, key);
                } catch {
                  fail();
                }
              };
            }
          }
        } catch {
          fail();
        }
      };
    });
  }

  async invalidateOrigin(origin: string): Promise<void> {
    // Deleting leases also fences late leaders from publishing an invalid catalogue.
    await this.transaction<void>((store, done, fail) => {
      const cursor = store.openCursor();
      cursor.onsuccess = () => {
        try {
          const row = cursor.result;
          if (!row) {
            done();
            return;
          }
          if (String(row.key).startsWith(origin + '|')) row.delete();
          row.continue();
        } catch {
          fail();
        }
      };
    }).catch(() => undefined);
  }

  async prepare(
    request: HttpPacket & { intent?: string },
    signal: AbortSignal,
    deadline: number,
    route?: CatalogueRoute,
    ownerAlive: () => boolean = () => true,
  ): Promise<CatalogueRead | undefined> {
    const key = this.policy.keyFor(request, route);
    if (!key) return;
    let owner: string;
    try {
      owner = crypto.randomUUID();
    } catch {
      if (this.policy.strict && this.coordinated) throw new HttpPreparationError('network');
      return;
    }
    const stopAt = Math.min(deadline, Date.now() + this.policy.leaseMs);
    for (;;) {
      signal.throwIfAborted();
      if (Date.now() >= deadline) throw new HttpPreparationError('timedOut');
      try {
        const entry = await this.edit(key, (entry) => {
          if (this.live(entry)) return { value: entry! };
          const next: Entry = {
            owner,
            at: Date.now(),
            ...(this.policy.strict
              ? { leaseUntil: Math.min(deadline, Date.now() + this.policy.leaseMs), failures: entry?.failures ?? 0 }
              : {}),
          };
          return { value: next, write: next };
        });
        if (signal.aborted || Date.now() >= deadline) {
          if (entry.owner === owner) this.leader(key, entry, signal, ownerAlive).cancel();
          signal.throwIfAborted();
          throw new HttpPreparationError('timedOut');
        }
        if (entry.bytes && !this.live(entry)) continue;
        if (entry.bytes && this.policy.valid(key, entry.bytes)) {
          const cached = { status: 200, headers: 'content-type: application/json', body: entry.bytes };
          return {
            cached,
            cachedAt: entry.at,
            expiresAt: entry.at + this.policy.ttlMs,
            complete: async () => cached,
            cancel: () => {},
          };
        }
        if (entry.owner === owner && entry.cooldownUntil === undefined)
          return this.leader(key, entry, signal, ownerAlive);
        if (entry.bytes)
          await this.edit(key, (current) => ({
            value: undefined,
            write: current?.owner === entry.owner ? null : undefined,
          }));
        if (!this.policy.strict && Date.now() >= stopAt) return;
        await delay(Math.min(POLL_MS, (this.policy.strict ? deadline : stopAt) - Date.now()), signal);
      } catch (error) {
        signal.throwIfAborted();
        if (error instanceof HttpPreparationError) throw error;
        if (this.policy.strict && this.coordinated) throw new HttpPreparationError('network');
        return;
      }
    }
  }

  private leader(key: string, lease: Entry, signal: AbortSignal, ownerAlive: () => boolean): CatalogueRead {
    const { owner } = lease;
    let finished = false;
    const remove = async () => {
      await this.edit(key, (entry) => ({ value: undefined, write: entry?.owner === owner ? null : undefined })).catch(
        () => undefined,
      );
    };
    const cancel = () => {
      if (finished) return;
      finished = true;
      signal.removeEventListener('abort', cancel);
      void remove();
    };
    const failed = async (headers = '', timedOut = false) => {
      if (!this.policy.failureDelay || finished || (signal.aborted && !timedOut)) {
        cancel();
        return;
      }
      finished = true;
      signal.removeEventListener('abort', cancel);
      const at = Date.now();
      const failures = Math.min(6, (lease.failures ?? 0) + 1);
      const until = at + this.policy.failureDelay(failures, headers, at);
      await this.edit(key, (entry) => ({
        value: undefined,
        write: ownerAlive() && entry?.owner === owner ? { owner, at, failures, cooldownUntil: until } : undefined,
      })).catch(() => undefined);
    };
    signal.addEventListener('abort', cancel, { once: true });
    return {
      cancel,
      ...(this.policy.failureDelay
        ? {
            failed,
            timedOut: () => {
              void failed('', true);
            },
          }
        : {}),
      ...(this.policy.strict ? { leaseExpiresAt: lease.leaseUntil, leaseStartedAt: lease.at } : {}),
      complete: async (response) => {
        try {
          if (response.status !== 200) {
            if (this.policy.failureDelay) await failed(response.headers);
            else cancel();
            return response;
          }
          const captured = await capture(response.body, signal, this.policy.maxBytes);
          if (!(captured instanceof Uint8Array) || !this.policy.valid(key, captured)) {
            if (this.policy.failureDelay) await failed(response.headers);
            else cancel();
            return { ...response, body: captured };
          }
          if (!finished && !signal.aborted) {
            finished = true;
            signal.removeEventListener('abort', cancel);
            const publicationBytes = captured.slice();
            const publishedAt = Date.now();
            // Cache publication owns its bytes and never delays a good HTTP answer.
            void this.edit(key, (entry) => ({
              value: undefined,
              write:
                ownerAlive() && entry?.owner === owner
                  ? { owner, at: publishedAt, bytes: publicationBytes }
                  : undefined,
            })).catch(() => undefined);
          }
          signal.throwIfAborted();
          return { ...response, body: captured };
        } catch (error) {
          if (this.policy.failureDelay) await failed();
          else cancel();
          throw error;
        }
      },
    };
  }
}

async function capture(
  body: HttpResponse['body'],
  signal: AbortSignal,
  maxBytes: number,
): Promise<HttpResponse['body']> {
  if (body instanceof Uint8Array) return body.length <= maxBytes ? body.slice() : body;
  const iterator = body[Symbol.asyncIterator]();
  const chunks: Uint8Array[] = [];
  let length = 0;
  let replay = false;
  try {
    for (;;) {
      signal.throwIfAborted();
      const next = await untilAborted(iterator.next(), signal);
      signal.throwIfAborted();
      if (next.done) break;
      length += next.value.length;
      if (length > maxBytes) {
        replay = true;
        return (async function* () {
          try {
            yield* chunks;
            yield next.value;
            for (;;) {
              const next = await iterator.next();
              if (next.done) return;
              yield next.value;
            }
          } finally {
            void iterator.return?.().catch(() => undefined);
          }
        })();
      }
      chunks.push(next.value.slice());
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.length;
    }
    return bytes;
  } finally {
    if (!replay) void iterator.return?.().catch(() => undefined);
  }
}

const untilAborted = <T>(pending: Promise<T>, signal: AbortSignal): Promise<T> =>
  new Promise((resolve, reject) => {
    const abort = () => reject(signal.reason);
    signal.addEventListener('abort', abort, { once: true });
    pending.then(
      (value) => {
        signal.removeEventListener('abort', abort);
        resolve(value);
      },
      (error) => {
        signal.removeEventListener('abort', abort);
        reject(error);
      },
    );
    if (signal.aborted) {
      signal.removeEventListener('abort', abort);
      reject(signal.reason);
    }
  });
