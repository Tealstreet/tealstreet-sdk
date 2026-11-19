import type { Position } from '../../generated/Position';

import { describe, expect, it, vi } from 'vitest';

import { PositionSide } from '../../generated/PositionSide';
import { FrameStore } from '../../store/frameStore';
import { nativeFixture } from '../cutover/nativeFixture';

const position: Position = {
  id: 'BTC-long',
  symbol: 'BTCUSDT',
  side: PositionSide.Long,
  contracts: 2,
  entryPrice: 100,
  notional: 200,
  unrealizedPnl: 3,
  leverage: 7,
  liquidationPrice: null,
  extra: { contractsNative: 20 },
};

describe('native closed positions', () => {
  it('removes omitted rows without manufacturing closed exposure', () => {
    const store = new FrameStore();
    store.ingest(nativeFixture({ positions: [position] }));
    const before = store.positions;
    const changed = vi.fn();
    store.subscribe('positions', changed);
    store.ingest(nativeFixture({ positions: [] }));
    store.ingest(nativeFixture({ positions: [] }));
    expect(store.positions).toEqual([]);
    expect(before).toEqual([position]);
    expect(changed).toHaveBeenCalledOnce();
  });

  it('preserves explicit zero rows, unknown liquidation and native contracts on reopening', () => {
    const store = new FrameStore();
    store.ingest(nativeFixture({ positions: [position] }));
    store.ingest(nativeFixture({ positions: [] }));
    store.ingest(nativeFixture({ positions: [{ ...position, contracts: 0 }] }));
    expect(store.positions).toEqual([{ ...position, contracts: 0 }]);
    expect(store.positions[0].liquidationPrice).toBeNull();
    expect(store.positions[0].extra?.contractsNative).toBe(20);
  });
});
