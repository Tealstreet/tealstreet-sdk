import type { StoreData } from '../../generated/StoreData';

import { FrameStore } from '../../store/frameStore';
import { nativeFixture } from './nativeFixture';

type EvidenceField = 'markets' | 'orders' | 'positions';
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
  data: Partial<StoreData>,
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
  // previously observed completion flags and other unchanged STATUS fields.
  const status = data.loaded !== undefined || data.options !== undefined || data.latency !== undefined;
  const frameData = status
    ? {
        ...data,
        loaded: { ...store.loaded, ...data.loaded },
        options: { ...store.options, ...data.options },
        latency: data.latency === undefined ? store.latency : data.latency,
      }
    : data;
  store.apply(nativeFixture(frameData, symbols));
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
