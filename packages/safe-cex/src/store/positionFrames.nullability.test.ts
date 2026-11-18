import type { Position } from '../types';

import { expect, it } from 'vitest';
import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { consumerFrame } from '../__tests__/cutover/consumerFrame';
import { PositionSide } from '../types';
import { DisplayedPositionsPostprocessor } from '../webworker/postProcessors/DisplayedPositionsPostprocessor';
import { FrameApplier } from './frameApplier';
import { DefaultStore } from './store.base';

const position = (liquidationPrice: number | null): Position => ({
  id: 'position-1', symbol: 'BTCUSDT', side: PositionSide.Long,
  entryPrice: 100, notional: 200, leverage: 2, unrealizedPnl: 0,
  contracts: 2, liquidationPrice,
});
const frame = (row: Position) => new Frame(consumerFrame({ positions: [row] }));

it('keeps liquidation null through apply and the displayedPositions selector used by the web', () => {
  const workerStore = new DefaultStore();
  new FrameApplier(workerStore).apply(frame(position(null)));
  expect(workerStore.positions[0].liquidationPrice).toBeNull();

  const output = workerStore.serialize();
  new DisplayedPositionsPostprocessor('test-account', 'bybit', { key: '', secret: '' }).process(output);
  expect(output.displayedPositions).toHaveLength(1);
  expect(output.displayedPositions[0].liquidationPrice).toBeNull();

  // The worker transfers this derived snapshot; OpenPositions selects
  // displayedPositions from the receiving account stores.
  const pageStore = new DefaultStore();
  pageStore.update(structuredClone(output), true);
  const selected = [pageStore].flatMap((store) => store.displayedPositions);
  expect(selected).toHaveLength(1);
  expect(selected[0].liquidationPrice).toBeNull();
});

it('preserves unknown liquidation through apply, a known price, clearing and a late snapshot', () => {
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  for (const price of [null, 50, null]) {
    const input = frame(position(price));
    expect(input.positions()?.liquidationPrice[0]).toBe(price);
    applier.apply(input);
    expect(store.positions[0].liquidationPrice).toBe(price);
    const snapshot = store.serialize();
    expect(JSON.parse(JSON.stringify(snapshot)).positions[0].liquidationPrice).toBe(price);
    const late = new DefaultStore();
    late.update(snapshot, true);
    expect(late.positions[0].liquidationPrice).toBe(price);
  }
});

it('keeps optional native numbers absent while preserving reported zero', () => {
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  const absent = position(null);
  applier.apply(frame(absent));
  for (const field of ['realizedPnl', 'marginSize', 'lastUpdate']) {
    expect(store.positions[0]).not.toHaveProperty(field);
  }
  applier.apply(frame({ ...absent, realizedPnl: 0, marginSize: 0, lastUpdate: 0 }));
  expect(store.positions[0]).toMatchObject({ liquidationPrice: null, realizedPnl: 0, marginSize: 0, lastUpdate: 0 });
});
