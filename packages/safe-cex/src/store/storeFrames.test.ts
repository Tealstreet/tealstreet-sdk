import { describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import fixtures from '../../../../sdk-rs/engine/tests/fixtures/store-frames.json';
import { MarginType, PositionMode } from '../types';
import { DefaultStore } from './store.base';
import { readStoreSettings, readStoreStatus } from './storeFrames';

const frame = (bytes: number[]) => new Frame(Uint8Array.from(bytes).buffer);

describe('generated Store frame module on the isolated Position base', () => {
  it('reads real native status/settings into the Store without repairing values', () => {
    const store = new DefaultStore();
    const emptyStatus = frame(fixtures.empty).status();
    if (!emptyStatus) throw new Error('status fixture required');
    store.update(readStoreStatus(emptyStatus));
    expect(store.latency).toBeNull();
    const ready = frame(fixtures.ready);
    const status = ready.status();
    const settings = ready.settings();
    if (!status || !settings) throw new Error('native Store fixture required');
    store.update({
      ...readStoreStatus(status),
      fetchedLeverage: readStoreSettings(settings, status.hedged, (_market, symbol) => symbol ?? ''),
    });
    expect(store.serialize().latency).toBe(0);
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
