import { describe, expect, it } from 'vitest';
import { scheduleMarketTwap } from './schedule';

const quantity = (atoms: string) => ({ atoms, decimal: atoms, atomScale: 0 });

describe('scheduleMarketTwap', () => {
  it('allocates an exact executable quantity across absolute slice times', () => {
    const result = scheduleMarketTwap({
      target: quantity('10'),
      sliceCount: 3,
      startAtMs: 1_000,
      durationMs: 1_000,
      constraints: { quantityStep: quantity('2'), minQuantity: quantity('2') },
    });

    expect(result).toEqual([
      { logicalUnitId: 'slice-0', scheduledAtMs: 1_000, requested: quantity('4'), quantity: quantity('4') },
      { logicalUnitId: 'slice-1', scheduledAtMs: 1_500, requested: quantity('4'), quantity: quantity('4') },
      { logicalUnitId: 'slice-2', scheduledAtMs: 2_000, requested: quantity('2'), quantity: quantity('2') },
    ]);
  });

  it('reports a slice below venue minimum instead of dropping or merging it', () => {
    const result = scheduleMarketTwap({
      target: quantity('10'),
      sliceCount: 3,
      startAtMs: 0,
      durationMs: 600,
      constraints: { quantityStep: quantity('2'), minQuantity: quantity('4') },
    });

    expect(result).toEqual([
      { logicalUnitId: 'slice-0', scheduledAtMs: 0, requested: quantity('4'), quantity: quantity('4') },
      { logicalUnitId: 'slice-1', scheduledAtMs: 300, requested: quantity('4'), quantity: quantity('4') },
      {
        logicalUnitId: 'slice-2',
        scheduledAtMs: 600,
        requested: quantity('2'),
        invalidReason: 'rounded quantity 2 is below venue minimum 4',
      },
    ]);
  });

  it('accepts a venue minimum that is not itself step-aligned', () => {
    expect(
      scheduleMarketTwap({
        target: quantity('4'),
        sliceCount: 1,
        startAtMs: 0,
        durationMs: 0,
        constraints: { quantityStep: quantity('2'), minQuantity: quantity('3') },
      }),
    ).toEqual([{ logicalUnitId: 'slice-0', scheduledAtMs: 0, requested: quantity('4'), quantity: quantity('4') }]);
  });

  it('rejects an absolute schedule outside the safe integer clock range', () => {
    expect(() =>
      scheduleMarketTwap({
        target: quantity('2'),
        sliceCount: 2,
        startAtMs: Number.MAX_SAFE_INTEGER,
        durationMs: 1,
        constraints: { quantityStep: quantity('1'), minQuantity: quantity('1') },
      }),
    ).toThrow(/absolute end time/);
  });

  it('rejects a target that cannot be represented by the venue quantity step', () => {
    expect(() =>
      scheduleMarketTwap({
        target: quantity('5'),
        sliceCount: 2,
        startAtMs: 0,
        durationMs: 100,
        constraints: { quantityStep: quantity('2'), minQuantity: quantity('2') },
      }),
    ).toThrow(/target quantity must be a multiple/);
  });
  it('allows a slice equal to the maximum and rejects one step above before placement', () => {
    expect(
      scheduleMarketTwap({
        target: quantity('10'),
        sliceCount: 2,
        startAtMs: 0,
        durationMs: 100,
        constraints: {
          quantityStep: quantity('1'),
          minQuantity: quantity('1'),
          maxQuantity: quantity('5'),
        },
      }),
    ).toEqual([
      { logicalUnitId: 'slice-0', scheduledAtMs: 0, requested: quantity('5'), quantity: quantity('5') },
      { logicalUnitId: 'slice-1', scheduledAtMs: 100, requested: quantity('5'), quantity: quantity('5') },
    ]);

    expect(
      scheduleMarketTwap({
        target: quantity('11'),
        sliceCount: 2,
        startAtMs: 0,
        durationMs: 100,
        constraints: {
          quantityStep: quantity('1'),
          minQuantity: quantity('1'),
          maxQuantity: quantity('5'),
        },
      }),
    ).toEqual([
      {
        logicalUnitId: 'slice-0',
        scheduledAtMs: 0,
        requested: quantity('6'),
        invalidReason: 'rounded quantity 6 exceeds venue maximum 5',
      },
      { logicalUnitId: 'slice-1', scheduledAtMs: 100, requested: quantity('5'), quantity: quantity('5') },
    ]);
  });
});
