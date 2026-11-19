import type { Position } from '../types';

import { expect, it } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { consumerFrame } from '../__tests__/cutover/consumerFrame';
import { nativeFixture } from '../__tests__/cutover/nativeFixture';
import { PositionSide } from '../types';
import { DisplayedPositionsPostprocessor } from '../webworker/postProcessors/DisplayedPositionsPostprocessor';
import { FrameStore } from './frameStore';

const position = (liquidationPrice: number | null): Position => ({
  id: 'position-1',
  symbol: 'BTCUSDT',
  side: PositionSide.Long,
  entryPrice: 100,
  notional: 200,
  leverage: 2,
  unrealizedPnl: 0,
  contracts: 2,
  liquidationPrice,
});
const frame = (row: Position) => new Frame(consumerFrame({ positions: [row] }));

it('keeps liquidation null through apply and the displayedPositions selector used by the web', () => {
  const workerStore = new FrameStore();
  workerStore.apply(frame(position(null)));
  expect(workerStore.positions[0].liquidationPrice).toBeNull();

  const output = workerStore.data;
  new DisplayedPositionsPostprocessor('test-account', 'bybit', { key: '', secret: '' }).process(output);
  expect(output.displayedPositions).toHaveLength(1);
  expect(output.displayedPositions[0].liquidationPrice).toBeNull();

  // The worker transfers this derived snapshot; OpenPositions selects
  // displayedPositions from the receiving account stores.
  const pageStore = new FrameStore();
  pageStore.apply(
    nativeFixture(
      structuredClone(output),
      pageStore.markets.map((row) => row.symbol),
    ),
  );
  const selected = [pageStore].flatMap((store) => store.displayedPositions);
  expect(selected).toHaveLength(1);
  expect(selected[0].liquidationPrice).toBeNull();
});

it('preserves unknown liquidation through apply, a known price, clearing and a late snapshot', () => {
  const store = new FrameStore();
  const applier = store;
  for (const price of [null, 50, null]) {
    const input = frame(position(price));
    expect(input.positions()?.liquidationPrice[0]).toBe(price);
    applier.apply(input);
    expect(store.positions[0].liquidationPrice).toBe(price);
    const snapshot = store.data;
    expect(JSON.parse(JSON.stringify(snapshot)).positions[0].liquidationPrice).toBe(price);
    const late = new FrameStore();
    late.apply(
      nativeFixture(
        snapshot,
        late.markets.map((row) => row.symbol),
      ),
    );
    expect(late.positions[0].liquidationPrice).toBe(price);
  }
});

it('keeps optional native numbers absent while preserving reported zero', () => {
  const store = new FrameStore();
  const applier = store;
  const absent = position(null);
  applier.apply(frame(absent));
  for (const field of ['realizedPnl', 'marginSize', 'lastUpdate']) {
    expect(store.positions[0]).not.toHaveProperty(field);
  }
  applier.apply(frame({ ...absent, realizedPnl: 0, marginSize: 0, lastUpdate: 0 }));
  expect(store.positions[0]).toMatchObject({ liquidationPrice: null, realizedPnl: 0, marginSize: 0, lastUpdate: 0 });
});
