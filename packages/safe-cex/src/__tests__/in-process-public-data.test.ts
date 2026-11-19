import { expect, it } from 'vitest';

import { BaseExchange } from '../exchanges/base';
import { InProcessPublicDataProvider } from '../exchanges/in-process-public-data';
import { FrameStore } from '../store/frameStore';

it('requires the native shared-feed transport rather than copying generated arrays into a second store', () => {
  const parent = new BaseExchange('bybit', { key: '', secret: '' }, new FrameStore());
  const source = new BaseExchange('bybit', { key: '', secret: '' }, new FrameStore());
  const provider = new InProcessPublicDataProvider(parent, source);
  const before = parent.store.data;
  try {
    expect(() => provider.connect()).toThrow('Rust shared-feed transport');
    expect(parent.store.data).toBe(before);
    expect(parent.store.loaded.markets).toBe(false);
  } finally {
    provider.dispose();
    parent.dispose();
    source.dispose();
  }
});
