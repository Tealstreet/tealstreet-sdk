import type { StoreData } from '../../generated/StoreData';

import { TICKER_FIELDS } from '@tealstreet/safe-cex-wasm/wire';
import { FrameStore } from '../../store/frameStore';
import { nativeFixture } from './nativeFixture';

type EvidenceField = 'markets' | 'orders' | 'positions';
const producerLoaded = new WeakMap<FrameStore, StoreData['loaded']>();
const evidence = new WeakMap<FrameStore, Record<EvidenceField, Map<string, unknown>>>();

/** A real native consumer seeded by an explicit retained protocol fixture. */
export function referenceStore(data: Partial<StoreData> = {}): FrameStore {
  const store = new FrameStore();
  applyReferenceFixture(store, data);
  return store;
}

/** Retained TS protocol oracles read raw REST evidence absent from SCXF. */
export function applyReferenceFixture(
  store: FrameStore,
  data: Partial<Omit<StoreData, 'loaded'>> & { loaded?: Partial<StoreData['loaded']> },
  symbols: readonly string[] = store.markets.map((row) => row.symbol),
): void {
  let retained = evidence.get(store);
  if (!retained) {
    retained = { markets: new Map(), orders: new Map(), positions: new Map() };
    evidence.set(store, retained);
    for (const field of ['markets', 'orders', 'positions'] as const) {
      const infos = retained[field];
      const cache = {
        source: store.data[field],
        infos: new Map<string, unknown>(),
        view: store.data[field].map((row) => row),
      };
      Object.defineProperty(store, field, {
        configurable: true,
        get: () => {
          const rows = store.data[field];
          if (
            cache.source !== rows ||
            cache.infos.size !== infos.size ||
            [...infos].some(([id, info]) => cache.infos.get(id) !== info)
          ) {
            cache.source = rows;
            cache.infos = new Map(infos);
            cache.view = rows.map((row) => (infos.has(row.id) ? { ...row, info: infos.get(row.id) } : row));
          }
          return cache.view;
        },
      });
    }
  }
  // STATUS is one packed section. A mode/latency-only fixture must carry its
  // previously reported completion flags and other unchanged STATUS fields.
  // Keep producer reports independently: consumer readiness is clamped until
  // all account sections decode, and is not source completion authority.
  const loaded = { ...(producerLoaded.get(store) ?? store.loaded), ...data.loaded };
  if (data.loaded) producerLoaded.set(store, loaded);
  const status = data.loaded !== undefined || data.options !== undefined || data.latency !== undefined;
  const frameData = status
    ? {
        ...data,
        loaded,
        options: { ...store.options, ...data.options },
        latency: data.latency === undefined ? store.latency : data.latency,
      }
    : { ...data, loaded: undefined };
  // High-volume references are native producers too: a sparse ticker change
  // should emit section 4 rather than re-encoding the whole catalogue per tick.
  const tickers = data.tickers;
  const previous = store.tickers;
  const sparse =
    data.markets === undefined &&
    tickers !== undefined &&
    store.hasTickerState &&
    tickers.length === previous.length &&
    tickers.every(
      (row, i) =>
        row.id === previous[i].id && row.symbol === previous[i].symbol && row.observedAt === previous[i].observedAt,
    );
  if (sparse) {
    const deltas: number[] = [];
    for (let i = 0; i < tickers.length; i++) {
      if (tickers[i] === previous[i]) continue;
      let mask = 0;
      const values: number[] = [];
      for (let field = 0; field < TICKER_FIELDS.length; field++) {
        const key = TICKER_FIELDS[field];
        if (Object.is(tickers[i][key], previous[i][key])) continue;
        mask |= 1 << field;
        values.push(tickers[i][key] ?? NaN);
      }
      if (mask) deltas.push(i, mask, ...values);
    }
    store.apply(nativeFixture({ ...frameData, tickers: undefined }, symbols, deltas));
  } else store.apply(nativeFixture(frameData, symbols));
  for (const field of ['markets', 'orders', 'positions'] as const) {
    const rows = data[field];
    if (!rows) continue;
    const infos = retained[field];
    infos.clear();
    for (const row of rows) if (row.info !== undefined) infos.set(row.id, row.info);
  }
  // Retained signing oracles also need their raw REST account configuration.
  // These getters exist only on the test instance; SCXF financial state stays decoded.
  for (const field of ['starkConfig', 'zkLinkConfig', 'userData'] as const) {
    if (data[field] === undefined) continue;
    const value = data[field];
    Object.defineProperty(store, field, { configurable: true, get: () => value });
  }
}
