import { describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { nativeFixture } from '../__tests__/cutover/nativeFixture';
import fixtures from '../../../../sdk-rs/engine/tests/fixtures/store-frames.json';
import { MarginType, PositionMode } from '../types';
import { FrameStore } from './frameStore';
import { readStoreSettings, readStoreStatus } from './storeFrames';

const frame = (bytes: number[]) => new Frame(Uint8Array.from(bytes).buffer);

describe('generated FrameStore frame module on the isolated Position base', () => {
  it('reads real native status/settings into the FrameStore without repairing values', () => {
    const store = new FrameStore();
    const emptyStatus = frame(fixtures.empty).status();
    if (!emptyStatus) throw new Error('status fixture required');
    store.apply(
      nativeFixture(
        readStoreStatus(emptyStatus),
        store.markets.map((row) => row.symbol),
      ),
    );
    expect(store.latency).toBeNull();
    const ready = frame(fixtures.ready);
    const status = ready.status();
    const settings = ready.settings();
    if (!status || !settings) throw new Error('native FrameStore fixture required');
    store.apply(
      nativeFixture(
        {
          ...readStoreStatus(status),
          fetchedLeverage: readStoreSettings(settings, status.hedged, (_market, symbol) => symbol ?? ''),
        },
        store.markets.map((row) => row.symbol),
      ),
    );
    expect(store.data.latency).toBe(0);
    expect(store.fetchedLeverage.BTCUSDT).toEqual({
      buyLeverage: 15,
      sellLeverage: 30,
      marginType: MarginType.Cross,
      positionMode: PositionMode.Hedged,
    });
    expect(store.fetchedLeverage.ZERO).toMatchObject({ buyLeverage: 0, sellLeverage: 0 });
    expect(store.fetchedLeverage).not.toHaveProperty('UNKNOWN');
  });

  it('refuses a malformed native rail', () => {
    const settings = frame(fixtures.ready).settings();
    if (!settings?.buyLeverage) throw new Error('settings fixture required');
    settings.buyLeverage[0] = NaN;
    expect(() => readStoreSettings(settings, true, (_market, symbol) => symbol ?? '')).toThrow(
      'Invalid native Store leverage rails',
    );
  });
});
