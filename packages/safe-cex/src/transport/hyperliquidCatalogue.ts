import type { HttpPacket, HttpResponse } from '@tealstreet/safe-cex-wasm/host';

import { HttpPreparationError } from '@tealstreet/safe-cex-wasm/host';

const TTL_MS = 5000;
const LEASE_MS = 3000;
const POLL_MS = 100;
const STORAGE_MS = 250;
const MAX_BYTES = 1_048_576;
const MAX_ENTRIES = 64;
type Entry = { owner: string; at: number; bytes?: Uint8Array };
type Decision<T> = { value: T; write?: Entry | null };
export interface CatalogueRead {
  cached?: HttpResponse;
  complete(response: HttpResponse): Promise<HttpResponse>;
  cancel(): void;
}

function keyFor(request: HttpPacket & { intent?: string }): string | undefined {
  if (request.intent !== 'public' || request.method !== 'POST') return;
  let url: URL;
  try {
    url = new URL(request.url);
  } catch {
    return;
  }
  if (url.pathname !== '/info' || url.search || url.hash || url.protocol !== 'https:' || url.username || url.password)
    return;
  if (Object.keys(request.headers).some((name) => !['content-type', 'accept'].includes(name.toLowerCase()))) return;
  try {
    const body = JSON.parse(request.body ?? 'null');
    if (!body || !['metaAndAssetCtxs', 'spotMetaAndAssetCtxs', 'perpDexs'].includes(body.type)) return;
    if (Object.keys(body).some((name) => name !== 'type' && !(name === 'dex' && body.type === 'metaAndAssetCtxs')))
      return;
    if (body.dex !== undefined && (typeof body.dex !== 'string' || !/^[a-zA-Z0-9_-]{0,64}$/.test(body.dex))) return;
    return `${url.origin}|${body.type}|${body.dex ?? ''}`;
  } catch {
    return;
  }
}
const live = (entry?: Entry): boolean =>
  Boolean(
    entry &&
    Number.isFinite(entry.at) &&
    Date.now() >= entry.at &&
    Date.now() < entry.at + (entry.bytes ? TTL_MS : LEASE_MS),
  );
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const decimals = (value: unknown): boolean => Number.isInteger(value) && Number(value) >= 0 && Number(value) <= 18;
const index = (value: unknown): boolean =>
  Number.isInteger(value) && Number(value) >= 0 && Number(value) <= 0xffff_ffff;
const numeric = (value: unknown): boolean =>
  (typeof value === 'number' || (typeof value === 'string' && value.trim() !== '')) && Number.isFinite(Number(value));
// Cache eligibility is conservative; native decoding still owns catalogue semantics.
const valid = (key: string, bytes: Uint8Array): boolean => {
  if (bytes.length > MAX_BYTES) return false;
  try {
    const value = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes));
    if (key.includes('|perpDexs|'))
      return (
        Array.isArray(value) &&
        value[0] === null &&
        value.slice(1).every((row) => record(row) && typeof row.name === 'string')
      );
    if (
      !Array.isArray(value) ||
      value.length !== 2 ||
      !value[0] ||
      !Array.isArray(value[0].universe) ||
      !Array.isArray(value[1])
    )
      return false;
    const spot = key.includes('|spotMetaAndAssetCtxs|');
    if (
      spot &&
      (!Array.isArray(value[0].tokens) ||
        !value[0].tokens.every(
          (row: unknown) => record(row) && typeof row.name === 'string' && index(row.index) && decimals(row.szDecimals),
        ))
    )
      return false;
    return (
      value[0].universe.length === value[1].length &&
      value[0].universe.every(
        (row: unknown) =>
          record(row) &&
          typeof row.name === 'string' &&
          (spot
            ? index(row.index) && Array.isArray(row.tokens) && row.tokens.length === 2 && row.tokens.every(index)
            : decimals(row.szDecimals) && (row.maxLeverage === undefined || numeric(row.maxLeverage))),
      ) &&
      value[1].every(
        (row: unknown) =>
          record(row) &&
          ['markPx', 'oraclePx', 'prevDayPx', 'dayNtlVlm', 'dayBaseVlm', 'openInterest', 'funding'].every(
            (field) => row[field] === undefined || numeric(row[field]),
          ) &&
          (row.midPx == null || numeric(row.midPx)),
      )
    );
  } catch {
    return false;
  }
};
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
export class HyperliquidCatalogue {
  private database?: Promise<IDBDatabase>;
  private retryAt = 0;
  constructor(private readonly factory?: IDBFactory) {}

  private open(): Promise<IDBDatabase> {
    if (this.retryAt > Date.now()) return Promise.reject(new Error('Catalogue cache unavailable'));
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
        const request = (this.factory ?? indexedDB).open('safe-cex-hyperliquid-catalogue', 1);
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
                    if (!live(row.value)) row.delete();
                    else count += 1;
                    row.continue();
                  } else if (count >= MAX_ENTRIES) fail();
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
  ): Promise<CatalogueRead | undefined> {
    const key = keyFor(request);
    if (!key) return;
    let owner: string;
    try {
      owner = crypto.randomUUID();
    } catch {
      return;
    }
    const stopAt = Math.min(deadline, Date.now() + LEASE_MS);
    for (;;) {
      signal.throwIfAborted();
      if (Date.now() >= deadline) throw new HttpPreparationError('timedOut');
      try {
        const entry = await this.edit(key, (entry) => {
          if (live(entry)) return { value: entry! };
          const next: Entry = { owner, at: Date.now() };
          return { value: next, write: next };
        });
        if (signal.aborted || Date.now() >= deadline) {
          if (entry.owner === owner) this.leader(key, owner, signal).cancel();
          signal.throwIfAborted();
          throw new HttpPreparationError('timedOut');
        }
        if (entry.bytes && valid(key, entry.bytes)) {
          const cached = { status: 200, headers: 'content-type: application/json', body: entry.bytes };
          return { cached, complete: async () => cached, cancel: () => {} };
        }
        if (entry.owner === owner) return this.leader(key, owner, signal);
        if (entry.bytes)
          await this.edit(key, (current) => ({
            value: undefined,
            write: current?.owner === entry.owner ? null : undefined,
          }));
        if (Date.now() >= stopAt) return;
        await delay(Math.min(POLL_MS, stopAt - Date.now()), signal);
      } catch (error) {
        signal.throwIfAborted();
        if (error instanceof HttpPreparationError) throw error;
        return;
      }
    }
  }

  private leader(key: string, owner: string, signal: AbortSignal): CatalogueRead {
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
    signal.addEventListener('abort', cancel, { once: true });
    return {
      cancel,
      complete: async (response) => {
        try {
          if (response.status !== 200) {
            cancel();
            return response;
          }
          const captured = await capture(response.body, signal);
          if (!(captured instanceof Uint8Array) || !valid(key, captured)) {
            cancel();
            return { ...response, body: captured };
          }
          if (!finished && !signal.aborted) {
            finished = true;
            signal.removeEventListener('abort', cancel);
            // Cache publication must never delay a good HTTP answer to its deadline.
            void this.edit(key, (entry) => ({
              value: undefined,
              write: entry?.owner === owner ? { owner, at: Date.now(), bytes: captured } : undefined,
            })).catch(() => undefined);
          }
          signal.throwIfAborted();
          return { ...response, body: captured };
        } catch (error) {
          cancel();
          throw error;
        }
      },
    };
  }
}

async function capture(body: HttpResponse['body'], signal: AbortSignal): Promise<HttpResponse['body']> {
  if (body instanceof Uint8Array) return body.length <= MAX_BYTES ? body.slice() : body;
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
      if (length > MAX_BYTES) {
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

export const sharedHyperliquidCatalogue = new HyperliquidCatalogue();
