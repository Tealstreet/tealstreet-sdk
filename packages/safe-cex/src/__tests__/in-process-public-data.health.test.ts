import { expect, it } from 'vitest';

import { FrameStore } from '../store/frameStore';
import { nativeFixture } from './cutover/nativeFixture';

it('publishes shared-feed health only from native socket-health sections', () => {
  const store = new FrameStore();
  const health = {
    publicHealthy: true,
    privateHealthy: false,
    publicInitialized: true,
    privateInitialized: false,
    publicLastMessage: 123,
  };
  store.ingest(nativeFixture({ socketHealth: health }));
  const observed = store.socketHealth;
  store.ingest(nativeFixture({ balance: { used: 0, free: 1, total: 1, upnl: 0 } }));
  expect(store.socketHealth).toBe(observed);
  expect(observed).toEqual(health);
  store.ingest(nativeFixture({ socketHealth: { ...health, publicHealthy: false } }));
  expect(store.socketHealth?.publicHealthy).toBe(false);
  expect(observed?.publicHealthy).toBe(true);
});
