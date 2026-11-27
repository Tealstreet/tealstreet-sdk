import type { ExchangeOptions } from '../../types';

import { withoutDelegatedSigning } from './delegatedSignerSetup';

const PAGE_CANDLE_SOURCE_LINGER_MS = 30_000;
const PAGE_CANDLE_REBUILD_MIN_MS = 2_000;
const PAGE_CANDLE_REBUILD_MAX_MS = 60_000;

type Disposable = { dispose(): void };
type Entry<S extends Disposable> = {
  source?: S;
  holders: Set<object>;
  linger?: ReturnType<typeof setTimeout>;
  rebuilds: number;
  rebuildAt: number;
};

/** One mirror's hold on its realm's shared page candle source. */
export interface PageCandleLease<S extends Disposable> {
  /** The live source, built on first use; refuses during a rebuild backoff. */
  source(build: () => S): S;
  succeeded(): void;
  /** A broken source is retired and rebuilt after a shared, doubling backoff. */
  failed(source: S, broken: boolean): void;
  release(): void;
}

/**
 * Page candle sources are credential-free public engines, shared per JavaScript realm and venue
 * configuration so replaced mirrors and several accounts run one market discovery (AGT-299).
 * The last release keeps the source briefly, so a mirror replacement reuses it.
 */
export class PageCandleSources<S extends Disposable> {
  private readonly entries = new Map<string, Entry<S>>();

  hold(key: string, holder: object): PageCandleLease<S> {
    let entry = this.entries.get(key);
    if (!entry) this.entries.set(key, (entry = { holders: new Set(), rebuilds: 0, rebuildAt: 0 }));
    const held = entry;
    clearTimeout(held.linger);
    held.linger = undefined;
    held.holders.add(holder);
    let released = false;
    return {
      source: (build) => {
        if (released) throw new Error('Page candle source lease was released');
        if (held.source) return held.source;
        if (Date.now() < held.rebuildAt) throw new PageCandleSourceRecovering();
        return (held.source = build());
      },
      succeeded: () => {
        held.rebuilds = 0;
      },
      failed: (source, broken) => {
        if (!broken || held.source !== source) return;
        held.source = undefined;
        source.dispose();
        held.rebuildAt =
          Date.now() + Math.min(PAGE_CANDLE_REBUILD_MIN_MS * 2 ** held.rebuilds, PAGE_CANDLE_REBUILD_MAX_MS);
        held.rebuilds += 1;
      },
      release: () => {
        if (released) return;
        released = true;
        held.holders.delete(holder);
        if (held.holders.size || held.linger) return;
        held.linger = setTimeout(() => {
          if (held.holders.size || this.entries.get(key) !== held) return;
          this.entries.delete(key);
          held.source?.dispose();
          held.source = undefined;
        }, PAGE_CANDLE_SOURCE_LINGER_MS);
      },
    };
  }

  /** Disposes every source now, including lingering ones. */
  dispose(): void {
    for (const entry of this.entries.values()) {
      clearTimeout(entry.linger);
      entry.source?.dispose();
    }
    this.entries.clear();
  }
}

export class PageCandleSourceRecovering extends Error {}

// Account identity and signing material never reach a shared public source or its key.
const ACCOUNT_FIELDS = [
  'signingMode',
  'auth',
  'paradexAddress',
  'subkeyAddress',
  'grvtSignerAddress',
  'grvtTradingAccountId',
  'starkPrivateKey',
  'vault',
  'applicationId',
  'apiKeyIndex',
  'subaccount',
  'hyperliquidBuilder',
  'tuning',
  'callerReportsPlacementFailures',
] as const;

/** The credential-free options every mirror's page candle source is built from. */
export function pageCandleSourceOptions(options: ExchangeOptions): ExchangeOptions {
  const routing: Record<string, unknown> = {
    ...withoutDelegatedSigning(options),
    workerMirror: false,
    publicDataOnly: true,
    privateOnly: false,
    publicFeed: undefined,
    externalPublicData: undefined,
  };
  for (const field of ACCOUNT_FIELDS) delete routing[field];
  if (options.polymarket) routing.polymarket = { marketSlugs: options.polymarket.marketSlugs };
  // BaseExchange's legacy type requires key/secret; the public source holds neither.
  return routing as ExchangeOptions;
}

const identities = new WeakMap<object, number>();
let nextIdentity = 0;

/** Plain values by content; host objects such as a transport provider by identity. */
export function pageCandleSourceKey(exchange: string, options: ExchangeOptions): string {
  const canonical = (value: unknown): unknown => {
    if (typeof value === 'function' || (value && typeof value === 'object' && !isPlain(value))) {
      const object = value as object;
      if (!identities.has(object)) identities.set(object, ++nextIdentity);
      return { identity: identities.get(object) };
    }
    if (Array.isArray(value)) return value.map(canonical);
    if (value && typeof value === 'object')
      return Object.fromEntries(
        Object.keys(value)
          .filter((key) => (value as Record<string, unknown>)[key] !== undefined)
          .sort()
          .map((key) => [key, canonical((value as Record<string, unknown>)[key])]),
      );
    return value;
  };
  return JSON.stringify([exchange, canonical(options)]);
}

const isPlain = (value: object) => {
  if (Array.isArray(value)) return true;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
};
