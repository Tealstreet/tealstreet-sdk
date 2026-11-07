import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { WOO_V3_MIGRATION_GATES } from '../reference/woo/woo.v3.contracts';
import {
  WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES,
  WooV3OrderBookRecoveryCoordinator,
} from '../reference/woo/woo.v3.orderbook-recovery';

const snapshot = (timestamp = 100) => ({
  success: true,
  timestamp,
  data: {
    bids: [
      { price: '100', quantity: '1' },
      { price: '99', quantity: '2' },
    ],
    asks: [{ price: '101', quantity: '1' }],
  },
});

const snapshotAtDepth = (depth: number, timestamp = 100) => ({
  success: true,
  timestamp,
  data: {
    bids: Array.from({ length: depth }, (_, index) => ({ price: `${1_000 - index}`, quantity: '1' })),
    asks: Array.from({ length: depth }, (_, index) => ({ price: `${1_001 + index}`, quantity: '1' })),
  },
});

const delta = (
  previousTimestamp: number,
  timestamp: number,
  overrides: Partial<{
    symbol: string;
    bids: readonly (readonly [string, string])[];
    asks: readonly (readonly [string, string])[];
  }> = {},
) => ({
  symbol: 'PERP_BTC_USDT',
  previousTimestamp,
  timestamp,
  bids: [],
  asks: [],
  ...overrides,
});

const createRecovery = (depth: 50 | 200 | 500 = 50) => new WooV3OrderBookRecoveryCoordinator('PERP_BTC_USDT', depth);

const acceptSnapshot = (recovery: WooV3OrderBookRecoveryCoordinator, input = snapshot()) =>
  recovery.acceptRestSnapshot(input, recovery.beginSnapshotRequest());

describe('WOO V3 inert orderbook recovery coordinator', () => {
  it('buffers without publication, discards stale pre-join deltas, and publishes only after an exact join', () => {
    const recovery = createRecovery();

    expect(recovery.pushUpdate(delta(98, 100, { bids: [['98', '9']] }))).toEqual({
      status: 'buffered',
      bufferedUpdates: 1,
      publication: undefined,
    });
    expect(
      recovery.pushUpdate(
        delta(100, 101, {
          bids: [
            ['100', '0'],
            ['100.5', '3'],
          ],
        }),
      ),
    ).toEqual({
      status: 'buffered',
      bufferedUpdates: 2,
      publication: undefined,
    });
    expect(
      recovery.pushUpdate(
        delta(101, 102, {
          asks: [
            ['101', '0'],
            ['102', '4'],
          ],
        }),
      ),
    ).toEqual({
      status: 'buffered',
      bufferedUpdates: 3,
      publication: undefined,
    });

    const result = acceptSnapshot(recovery);
    expect(result).toMatchObject({
      status: 'joined',
      snapshotTimestamp: 100,
      updateTimestamp: 102,
      appliedUpdates: 2,
      staleUpdatesDiscarded: 1,
      duplicatesIgnored: 0,
    });
    expect(result.publication).toEqual({
      snapshotTimestamp: 100,
      timestamp: 102,
      bids: [
        { price: 100.5, amount: 3, total: 3 },
        { price: 99, amount: 2, total: 5 },
      ],
      asks: [{ price: 102, amount: 4, total: 4 }],
    });
    expect(recovery.requiresFreshSnapshot).toBe(false);
  });

  it('preserves the mapped REST timestamp while waiting for a future exact join', () => {
    const recovery = createRecovery();

    expect(acceptSnapshot(recovery, snapshot(1_725_000_000_000))).toEqual({
      status: 'awaiting-join',
      snapshotTimestamp: 1_725_000_000_000,
      staleUpdatesDiscarded: 0,
      publication: undefined,
    });

    const joined = recovery.pushUpdate(delta(1_725_000_000_000, 1_725_000_000_001));
    expect(joined).toMatchObject({
      status: 'joined',
      snapshotTimestamp: 1_725_000_000_000,
      updateTimestamp: 1_725_000_000_001,
    });
    expect(joined.publication).toMatchObject({
      snapshotTimestamp: 1_725_000_000_000,
      timestamp: 1_725_000_000_001,
    });
  });

  it('accepts only the newest snapshot request token and consumes it exactly once', () => {
    const recovery = createRecovery();
    expect(recovery.acceptRestSnapshot(snapshot(), undefined)).toEqual({
      status: 'ignored',
      reason: 'stale-snapshot-request',
      publication: undefined,
    });
    const requestA = recovery.beginSnapshotRequest();
    const requestB = recovery.beginSnapshotRequest();

    expect(requestB).toBeGreaterThan(requestA);
    expect(recovery.acceptRestSnapshot(snapshot(), requestA)).toEqual({
      status: 'ignored',
      reason: 'stale-snapshot-request',
      publication: undefined,
    });
    expect(recovery.acceptRestSnapshot(snapshot(), requestB)).toMatchObject({
      status: 'awaiting-join',
      snapshotTimestamp: 100,
    });
    expect(recovery.acceptRestSnapshot(snapshot(), requestB)).toEqual({
      status: 'ignored',
      reason: 'stale-snapshot-request',
      publication: undefined,
    });
  });

  it('bounds each delta by its configured stream depth', () => {
    const levelsAtDepth = Array.from({ length: 50 }, (_, index) => [`${100 + index}`, '1'] as const);
    const atDepth = createRecovery(50);
    expect(atDepth.pushUpdate(delta(1, 2, { bids: levelsAtDepth, asks: levelsAtDepth }))).toEqual({
      status: 'buffered',
      bufferedUpdates: 1,
      publication: undefined,
    });

    const beyondDepth = createRecovery(50);
    expect(beyondDepth.pushUpdate(delta(1, 2, { bids: [...levelsAtDepth, ['1000', '1']] }))).toEqual({
      status: 'recover',
      reason: 'frame-level-overflow',
      requiresFreshSnapshot: true,
      publication: undefined,
    });
  });

  it.each([50, 200, 500] as const)(
    'accepts an exact-depth %i REST snapshot and rejects depth plus one before seeding',
    (depth) => {
      const atDepth = createRecovery(depth);
      expect(acceptSnapshot(atDepth, snapshotAtDepth(depth))).toMatchObject({
        status: 'awaiting-join',
        snapshotTimestamp: 100,
        publication: undefined,
      });

      const beyondDepth = createRecovery(depth);
      beyondDepth.pushUpdate(delta(100, 101));
      const snapshotRequest = beyondDepth.beginSnapshotRequest();
      expect(beyondDepth.acceptRestSnapshot(snapshotAtDepth(depth + 1), snapshotRequest)).toEqual({
        status: 'recover',
        reason: 'frame-level-overflow',
        requiresFreshSnapshot: true,
        publication: undefined,
      });
      expect(beyondDepth.acceptRestSnapshot(snapshotAtDepth(depth), snapshotRequest)).toEqual({
        status: 'ignored',
        reason: 'stale-snapshot-request',
        publication: undefined,
      });
      expect(beyondDepth.pushUpdate(delta(200, 201))).toEqual({
        status: 'buffered',
        bufferedUpdates: 1,
        publication: undefined,
      });
    },
  );

  it('buffers at the exact queue bound and fails closed on bound plus one', () => {
    const recovery = createRecovery();
    const outstandingSnapshotRequest = recovery.beginSnapshotRequest();
    let result;
    for (let index = 0; index < WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES; index++) {
      result = recovery.pushUpdate(delta(index + 1, index + 2));
    }
    expect(result).toEqual({
      status: 'buffered',
      bufferedUpdates: WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES,
      publication: undefined,
    });

    expect(
      recovery.pushUpdate(delta(WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES + 1, WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES + 2)),
    ).toEqual({
      status: 'recover',
      reason: 'buffer-overflow',
      requiresFreshSnapshot: true,
      publication: undefined,
    });
    expect(recovery.acceptRestSnapshot(snapshot(), outstandingSnapshotRequest)).toEqual({
      status: 'ignored',
      reason: 'stale-snapshot-request',
      publication: undefined,
    });
    expect(recovery.pushUpdate(delta(1, 2))).toEqual({
      status: 'buffered',
      bufferedUpdates: 1,
      publication: undefined,
    });
    expect(recovery.beginSnapshotRequest()).toBeGreaterThan(outstandingSnapshotRequest);
  });

  it('fails closed when buffered or future deltas cannot join the snapshot timestamp', () => {
    const bufferedGap = createRecovery();
    bufferedGap.pushUpdate(delta(101, 102));
    expect(acceptSnapshot(bufferedGap)).toEqual({
      status: 'recover',
      reason: 'missing-join',
      requiresFreshSnapshot: true,
      publication: undefined,
    });

    const futureGap = createRecovery();
    acceptSnapshot(futureGap);
    expect(futureGap.pushUpdate(delta(101, 102))).toEqual({
      status: 'recover',
      reason: 'missing-join',
      requiresFreshSnapshot: true,
      publication: undefined,
    });
  });

  it('clears a joined book on a chain gap and buffers again until a fresh snapshot joins', () => {
    const recovery = createRecovery();
    recovery.pushUpdate(delta(100, 101));
    acceptSnapshot(recovery);

    expect(recovery.pushUpdate(delta(100, 102))).toEqual({
      status: 'recover',
      reason: 'sequence-gap',
      requiresFreshSnapshot: true,
      publication: undefined,
    });
    expect(recovery.requiresFreshSnapshot).toBe(true);
    expect(recovery.pushUpdate(delta(200, 201))).toEqual({
      status: 'buffered',
      bufferedUpdates: 1,
      publication: undefined,
    });
    expect(acceptSnapshot(recovery, snapshot(200))).toMatchObject({
      status: 'joined',
      updateTimestamp: 201,
    });
  });

  it('resets on reconnect and never republishes the pre-reconnect book', () => {
    const recovery = createRecovery();
    recovery.pushUpdate(delta(100, 101));
    const oldRequest = recovery.beginSnapshotRequest();
    expect(recovery.acceptRestSnapshot(snapshot(), oldRequest)).toMatchObject({ status: 'joined' });

    const pendingRequest = recovery.beginSnapshotRequest();
    expect(recovery.reconnect()).toEqual({
      status: 'reset',
      reason: 'reconnect',
      requiresFreshSnapshot: true,
      publication: undefined,
    });
    expect(recovery.pushUpdate(delta(101, 102))).toEqual({
      status: 'buffered',
      bufferedUpdates: 1,
      publication: undefined,
    });
    expect(recovery.acceptRestSnapshot(snapshot(101), pendingRequest)).toEqual({
      status: 'ignored',
      reason: 'stale-snapshot-request',
      publication: undefined,
    });
  });

  it('ignores exact duplicates and old joined updates but rejects conflicting duplicates and pre-join reordering', () => {
    const recovery = createRecovery();
    const first = delta(100, 101, { bids: [['100', '2']] });
    recovery.pushUpdate(first);
    expect(recovery.pushUpdate(first)).toEqual({
      status: 'ignored',
      reason: 'duplicate',
      updateTimestamp: 101,
      publication: undefined,
    });
    acceptSnapshot(recovery);
    expect(recovery.pushUpdate(first)).toEqual({
      status: 'ignored',
      reason: 'duplicate',
      updateTimestamp: 101,
      publication: undefined,
    });
    expect(recovery.pushUpdate(delta(98, 99))).toEqual({
      status: 'ignored',
      reason: 'stale',
      updateTimestamp: 99,
      publication: undefined,
    });

    const conflicting = createRecovery();
    conflicting.pushUpdate(first);
    expect(conflicting.pushUpdate(delta(100, 101, { bids: [['100', '3']] }))).toEqual({
      status: 'recover',
      reason: 'conflicting-duplicate',
      requiresFreshSnapshot: true,
      publication: undefined,
    });

    const reordered = createRecovery();
    reordered.pushUpdate(delta(101, 102));
    expect(reordered.pushUpdate(delta(100, 101))).toEqual({
      status: 'recover',
      reason: 'out-of-order-prejoin-update',
      requiresFreshSnapshot: true,
      publication: undefined,
    });
  });

  it.each([
    ['zero timestamp', delta(0, 1)],
    ['non-increasing timestamps', delta(2, 2)],
    ['unsafe timestamp', delta(1, Number.MAX_SAFE_INTEGER + 1)],
    ['wrong symbol', delta(1, 2, { symbol: 'PERP_ETH_USDT' })],
    ['numeric wire price', delta(1, 2, { bids: [[1 as unknown as string, '1']] })],
    ['zero price', delta(1, 2, { bids: [['0', '1']] })],
    ['negative quantity', delta(1, 2, { asks: [['1', '-1']] })],
    ['underflow quantity', delta(1, 2, { asks: [['1', `0.${'0'.repeat(400)}1`]] })],
    ['overflow price', delta(1, 2, { asks: [['9'.repeat(400), '1']] })],
  ])('rejects a %s delta and requires a fresh snapshot', (_name, input) => {
    const recovery = createRecovery();
    expect(recovery.pushUpdate(input)).toEqual({
      status: 'recover',
      reason: 'invalid-delta',
      requiresFreshSnapshot: true,
      publication: undefined,
    });
  });

  it('rejects a malformed REST snapshot without exposing its prior joined state', () => {
    const recovery = createRecovery();
    recovery.pushUpdate(delta(100, 101));
    acceptSnapshot(recovery);

    expect(acceptSnapshot(recovery, { ...snapshot(), timestamp: 0 })).toEqual({
      status: 'recover',
      reason: 'invalid-snapshot',
      requiresFreshSnapshot: true,
      publication: undefined,
    });
  });

  it('remains disconnected from active Woo runtime with every migration gate false', () => {
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);

    for (const runtimeFile of ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-public.ts', 'woo.ws-private.ts']) {
      const source = readFileSync(join(process.cwd(), 'src/__tests__/reference/woo', runtimeFile), 'utf8');
      expect(source).not.toContain('woo.v3.orderbook-recovery');
    }
  });
});
