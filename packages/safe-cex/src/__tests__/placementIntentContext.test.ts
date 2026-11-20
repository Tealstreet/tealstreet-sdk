import { describe, expect, it, vi } from 'vitest';

import { delegatedIntentTime } from '../delegatedOrderFrames';
import { capturePlacementIntent, runWithPlacementIntent } from '../placementIntentContext';

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
