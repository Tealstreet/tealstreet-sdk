import { describe, expect, it } from 'vitest';
import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import fixtures from '../../../../sdk-rs/engine/tests/fixtures/store-frames.json';
import { FrameStore } from './frameStore';

const frame = (bytes: number[]) => new Frame(Uint8Array.from(bytes).buffer);

describe('FrameStore native state', () => {
  it('retains unchanged section identities and versions across equal frames', () => {
    const store = new FrameStore();
    store.ingest(frame(fixtures.ready), { snapshot: true, seq: 10 });
    const before = store.data;
    const versions = store.versions;
    store.ingest(frame(fixtures.ready), { seq: 11 });
    expect(store.data).toBe(before);
    expect(store.versions).toEqual(versions);
    expect(store.hasMarketIdentity).toBe(true);
    expect(store.hasAccountState).toBe(true);
    expect(store.latency).toBe(0);
  });

  it('publishes typed section changes with independent versions', () => {
    const store = new FrameStore();
    const values: Array<number | null> = [];
    const stop = store.subscribe('latency', (value, version) => {
      values.push(value);
      expect(version).toBe(store.version('latency'));
    });
    store.ingest(frame(fixtures.ready));
    const markets = store.markets;
    store.ingest(frame(fixtures.empty));
    expect(values).toEqual([0, null]);
    expect(store.markets).toBe(markets);
    stop();
    store.ingest(frame(fixtures.ready));
    expect(values).toEqual([0, null]);
  });

  it('rejects gaps until a fresh snapshot establishes a new sequence domain', () => {
    const store = new FrameStore();
    store.ingest(frame(fixtures.ready), { snapshot: true, seq: 1 });
    const before = store.data;
    expect(() => store.ingest(frame(fixtures.empty), { seq: 3 })).toThrow('sequence gap');
    expect(store.data).toBe(before);
    expect(store.snapshotRequired).toBe(true);
    expect(store.hasMarketIdentity).toBe(false);
    expect(() => store.ingest(frame(fixtures.empty), { seq: 2 })).toThrow('sequence gap');
    store.ingest(frame(fixtures.ready), { snapshot: true, seq: 20 });
    expect(store.snapshotRequired).toBe(false);
    expect(store.hasMarketIdentity).toBe(true);
    expect(store.seq).toBe(20);
  });

  it('keeps valid peers when SETTINGS fails and distinguishes subscriber failures', () => {
    const store = new FrameStore();
    store.ingest(frame(fixtures.ready));
    const leverage = store.fetchedLeverage;
    const malformed = frame(fixtures.ready);
    const settings = malformed.settings();
    if (!settings?.buyLeverage) throw new Error('Fixture missing rails');
    settings.buyLeverage[0] = NaN;
    expect(() => store.ingest(malformed)).toThrow('Invalid native Store leverage rails');
    expect(store.fetchedLeverage).toBe(leverage);
    expect(store.hasAccountState).toBe(true);
    const observed: boolean[] = [];
    store.subscribe(() => { throw new Error('subscriber'); });
    store.subscribe(() => { observed.push(true); });
    expect(() => store.ingest(frame(fixtures.empty))).toThrow('subscriber failed');
    expect(observed).toEqual([true]);
    expect(store.snapshotRequired).toBe(false);
  });
});
