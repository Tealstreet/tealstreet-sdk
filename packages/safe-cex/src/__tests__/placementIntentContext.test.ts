import { describe, expect, it, vi } from 'vitest';

import { delegatedIntentTime } from '../delegatedOrderFrames';
import { capturePlacementIntent, mintPlacementIntent, placementLegIntent, runWithPlacementIntent } from '../placementIntentContext';

const first = '019a9b43-1000-7000-8000-000000000001';
const second = '019a9b43-1001-7000-8000-000000000002';

describe('placement intent propagation', () => {
  it('captures the canonical UUID and its own timestamp before a deferral', async () => {
    const captured = runWithPlacementIntent(first, capturePlacementIntent);
    expect(captured).toEqual({ id: first, createdAt: delegatedIntentTime(first) });
    expect(Object.isFrozen(captured)).toBe(true);
    await Promise.resolve();
    expect(capturePlacementIntent()).toBeUndefined();
    expect(runWithPlacementIntent(captured?.id, capturePlacementIntent)).toEqual(captured);
  });

  it('restores nested contexts without leaking identity to another action', () => {
    runWithPlacementIntent(first, () => {
      expect(capturePlacementIntent()?.id).toBe(first);
      runWithPlacementIntent(second, () => expect(capturePlacementIntent()?.id).toBe(second));
      expect(capturePlacementIntent()?.id).toBe(first);
      runWithPlacementIntent(undefined, () => expect(capturePlacementIntent()).toBeUndefined());
      expect(capturePlacementIntent()?.id).toBe(first);
    });
    expect(capturePlacementIntent()).toBeUndefined();
  });

  it('rejects invalid identities before any callback or command', () => {
    const send = vi.fn();
    for (const value of ['', 'invalid', first.toUpperCase(), '019a9b43-1000-4000-8000-000000000001'])
      expect(() => runWithPlacementIntent(value, send)).toThrow();
    expect(send).not.toHaveBeenCalled();
    expect(capturePlacementIntent()).toBeUndefined();
  });

  it('does not keep ambient identity across asynchronous work', async () => {
    await runWithPlacementIntent(first, async () => {
      await Promise.resolve();
      expect(capturePlacementIntent()).toBeUndefined();
    });
  });
});


it('mints separate canonical actions at the same synchronized time and stable child UUIDs', () => {
  const first = mintPlacementIntent(1_764_000_000_000);
  const second = mintPlacementIntent(first.createdAt);
  expect(second.id).not.toBe(first.id);
  expect(delegatedIntentTime(first.id)).toBe(first.createdAt);
  const stop = placementLegIntent(first, 'stop');
  expect(stop).toEqual(placementLegIntent(first, 'stop'));
  expect(stop.id).not.toBe(placementLegIntent(first, 'take-profit').id);
  expect(stop.id).not.toBe(first.id);
  expect(delegatedIntentTime(stop.id)).toBe(first.createdAt);
});
