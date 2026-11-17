import type { Position } from '../../generated/Position';

import { describe, expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { PositionSide } from '../../generated/PositionSide';
import { FrameStore } from '../../store/frameStore';
import { nativeStoreFrame } from '../cutover/nativeFixture';

const position = (id: string): Position => ({
  id,
  symbol: id,
  side: PositionSide.Long,
  contracts: 2,
  entryPrice: 100,
  notional: 200,
  unrealizedPnl: 3,
  leverage: 7,
  liquidationPrice: null,
});

describe('native position replacement across the worker boundary', () => {
  it('removes omitted positions after transferring exact native bytes', () => {
    const worker = new FrameStore();
    const main = new FrameStore();
    const deliver = (positions: Position[]) => {
      const buffer = nativeStoreFrame({ positions });
      worker.ingest(new Frame(buffer));
      const transferred = structuredClone(buffer, { transfer: [buffer] });
      expect(buffer.byteLength).toBe(0);
      main.ingest(new Frame(transferred));
    };
    deliver([position('BTC'), position('ETH')]);
    const before = main.positions;
    deliver([position('ETH')]);
    expect(main.positions.map((row) => row.id)).toEqual(['ETH']);
    expect(main.positions).toEqual(worker.positions);
    expect(before.map((row) => row.id)).toEqual(['BTC', 'ETH']);
    deliver([]);
    expect(main.positions).toEqual([]);
    expect(main.data.positions).toEqual(worker.data.positions);
  });

  it('keeps independent sections while an authoritative empty position table arrives', () => {
    const main = new FrameStore();
    main.ingest(
      new Frame(nativeStoreFrame({ positions: [position('BTC')], balance: { used: 1, free: 2, total: 3, upnl: 0 } })),
    );
    const balance = main.balance;
    const version = main.version('positions');
    main.ingest(new Frame(nativeStoreFrame({ positions: [] })));
    expect(main.balance).toBe(balance);
    expect(main.version('positions')).toBe(version + 1);
  });
});
