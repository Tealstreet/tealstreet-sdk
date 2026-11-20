import { describe, expect, it, vi } from 'vitest';

import { PlacementIntentLedger } from '../placementIntentLedger';

const intent = (number: number) => {
  const createdAt = 1_764_000_000_000 + number;
  const time = createdAt.toString(16).padStart(12, '0');
  return {
    id: `${time.slice(0, 8)}-${time.slice(8)}-7000-8000-${number.toString(16).padStart(12, '0')}`,
    createdAt,
  };
};
const order = JSON.stringify({ symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 1000 });

describe('placement intent identity', () => {
  it('sends three deliberate identical actions while all replies are pending', async () => {
    const ledger = new PlacementIntentLedger<string[]>();
    let finish = (_ids: string[]) => {};
    const pending = new Promise<string[]>((resolve) => { finish = resolve; });
    const send = vi.fn(() => pending);
    const actions = [1, 2, 3].map((number) => ledger.run(intent(number), order, send));
    expect(send).toHaveBeenCalledTimes(3);
    finish(['venue-order']);
    expect(await Promise.all(actions)).toEqual([['venue-order'], ['venue-order'], ['venue-order']]);
  });

  it('double-delivering one action dispatches one order and shares its receipt', async () => {
    const ledger = new PlacementIntentLedger<string[]>();
    let finish = (_ids: string[]) => {};
    const pending = new Promise<string[]>((resolve) => { finish = resolve; });
    const send = vi.fn(() => pending);
    const first = ledger.run(intent(1), order, send);
    const duplicate = ledger.run({ ...intent(1) }, order, send);
    finish(['venue-order']);
    expect(await Promise.all([first, duplicate])).toEqual([['venue-order'], ['venue-order']]);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it('replaying an Unknown intent retains its uncertainty without another send', async () => {
    const ledger = new PlacementIntentLedger<string[]>();
    const unknown = Object.assign(new Error('response lost'), { outcome: 'unknown' });
    const send = vi.fn<() => Promise<string[]>>().mockRejectedValueOnce(unknown).mockResolvedValue(['second-order']);
    await expect(ledger.run(intent(1), order, send)).rejects.toBe(unknown);
    await expect(ledger.run({ ...intent(1) }, order, send)).rejects.toBe(unknown);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it('refuses a changed payload under the same identity', async () => {
    const ledger = new PlacementIntentLedger<string[]>();
    const send = vi.fn(async () => ['venue-order']);
    await ledger.run(intent(1), order, send);
    await expect(ledger.run(intent(1), order + 'different', send)).rejects.toMatchObject({ code: 'intentConflict' });
    expect(send).toHaveBeenCalledTimes(1);
  });

  it('isolates identical UUIDs held by independent authority scopes', async () => {
    const first = new PlacementIntentLedger<string[]>();
    const other = new PlacementIntentLedger<string[]>();
    const send = vi.fn(async () => ['venue-order']);
    await Promise.all([first.run(intent(1), order, send), other.run(intent(1), order, send)]);
    expect(send).toHaveBeenCalledTimes(2);
  });

  it('bounds known history without dropping Unknowns to make room', async () => {
    // Rule 5: AGT-105 retention requires Unknowns to remain; only known records retire.
    const ledger = new PlacementIntentLedger<string[]>(2);
    const unknown = Object.assign(new Error('response lost'), { outcome: 'unknown' });
    await expect(ledger.run(intent(1), order, async () => { throw unknown; })).rejects.toBe(unknown);
    await ledger.run(intent(2), order, async () => ['two']);
    await ledger.run(intent(3), order, async () => ['three']);
    const resend = vi.fn(async () => ['duplicate']);
    await expect(ledger.run(intent(1), order, resend)).rejects.toBe(unknown);
    await expect(ledger.run(intent(2), order, resend)).rejects.toMatchObject({ code: 'intentRetired', outcome: 'unknown' });
    expect(resend).not.toHaveBeenCalled();
    expect(ledger.size).toBeLessThanOrEqual(2);
  });

  it('refuses capacity rather than discarding an unanswered or Unknown intent', async () => {
    const ledger = new PlacementIntentLedger<string[]>(1);
    const unknown = Object.assign(new Error('response lost'), { outcome: 'unknown' });
    await expect(ledger.run(intent(1), order, async () => { throw unknown; })).rejects.toBe(unknown);
    const send = vi.fn(async () => ['extra']);
    await expect(ledger.run(intent(2), order, send)).rejects.toMatchObject({ code: 'intentCapacity', outcome: 'notSent' });
    await expect(ledger.run(intent(1), order, send)).rejects.toBe(unknown);
    expect(send).not.toHaveBeenCalled();
  });

  it('does not advance its rejection floor into an incoming action clock rollback', async () => {
    const ledger = new PlacementIntentLedger<string[]>(1);
    await ledger.run(intent(100), order, async () => ['future-known']);
    const send = vi.fn(async () => ['extra']);
    await expect(ledger.run(intent(1), order, send)).rejects.toMatchObject({ code: 'intentCapacity', outcome: 'notSent' });
    expect(send).not.toHaveBeenCalled();
    await expect(ledger.run(intent(100), order, send)).resolves.toEqual(['future-known']);
  });

  it('never evicts an unanswered intent to admit another', async () => {
    const ledger = new PlacementIntentLedger<string[]>(1);
    let finish = (_ids: string[]) => {};
    const pending = new Promise<string[]>((resolve) => { finish = resolve; });
    const original = ledger.run(intent(1), order, () => pending);
    const send = vi.fn(async () => ['extra']);
    const next = ledger.run(intent(2), order, send);
    finish(['first']);
    await original;
    await expect(next).rejects.toMatchObject({ code: 'intentCapacity', outcome: 'notSent' });
    expect(send).not.toHaveBeenCalled();
  });
});
