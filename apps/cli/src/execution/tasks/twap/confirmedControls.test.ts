import { afterEach, describe, expect, it, vi } from 'vitest';

import { OrderSide } from '@tealstreet/safe-cex/src/types';

import type { MarketTwapReport } from './MarketTwapTask';
import {
  formatConfirmedFinalReport,
  killAllConfirmed,
  listConfirmedReports,
  oneShotExitCode,
} from './confirmedControls';
import { confirmedTwapHistory, confirmedTwapHosts } from './startConfirmed';

const quantity = (decimal: string, atoms = decimal.replace('.', '')) => ({
  decimal,
  atoms,
  atomScale: decimal.includes('.') ? decimal.length - decimal.indexOf('.') - 1 : 0,
});

const report = (overrides: Partial<MarketTwapReport> = {}): MarketTwapReport => ({
  taskId: 'ctwap-1',
  symbol: 'BTCUSDT',
  side: OrderSide.Buy,
  lifecycle: 'completed',
  confirmedFilled: quantity('0.01', '1'),
  reservedUnresolved: quantity('0.00', '0'),
  rejected: quantity('0.00', '0'),
  residual: quantity('0.00', '0'),
  slices: [],
  ...overrides,
});

afterEach(() => {
  confirmedTwapHistory.clear();
  confirmedTwapHosts.clear();
});

describe('confirmed task controls', () => {
  it('retains terminal reports after their empty host is removed', () => {
    const terminal = report();
    confirmedTwapHistory.set(terminal.taskId, terminal);

    expect(listConfirmedReports()).toEqual([terminal]);
  });

  it('fences every account before waiting for any host to settle', async () => {
    const first = Promise.withResolvers<readonly MarketTwapReport[]>();
    const stopFirst = vi.fn(() => first.promise);
    const stopSecond = vi.fn().mockResolvedValue([report({ taskId: 'ctwap-2' })]);
    confirmedTwapHosts.set('first', { stopAll: stopFirst } as never);
    confirmedTwapHosts.set('second', { stopAll: stopSecond } as never);

    const killed = killAllConfirmed();

    expect(stopFirst).toHaveBeenCalledOnce();
    expect(stopSecond).toHaveBeenCalledOnce();
    first.resolve([report()]);
    await expect(killed).resolves.toBe(2);
  });

  it.each([
    ['filled', report(), 0],
    ['command-independent partial', report({ lifecycle: 'partial', residual: quantity('0.01', '1') }), 2],
    ['stopped', report({ lifecycle: 'stopped' }), 2],
    ['unresolved', report({ lifecycle: 'stopping', reservedUnresolved: quantity('0.01', '1') }), 3],
  ])('maps %s reports to the one-shot exit contract', (_name, value, expected) => {
    expect(oneShotExitCode([value])).toBe(expected);
  });

  it('prints the logical, caller, wire, venue, status, total, fill, and task totals', () => {
    const handle = {
      identity: { ownerId: 'ctwap-1', unitId: 'slice-1' },
      fingerprint: 'fingerprint',
      marketId: 'BTCUSDT',
      symbol: 'BTCUSDT',
      callerId: 'caller-1',
      wireClientOrderId: 'wire-1',
      exact: quantity('0.01', '1'),
      placement: { kind: 'ambiguous' as const, reason: 'lost placement acknowledgement' },
    };
    const filled = report({
      slices: [
        {
          unitId: 'slice-1',
          scheduledAtMs: 1,
          requested: quantity('0.01', '1'),
          truth: {
            kind: 'terminal',
            handle,
            observation: {
              status: 'filled',
              venueOrderId: 'venue-1',
              total: quantity('0.01', '1'),
              filled: quantity('0.01', '1'),
              residual: quantity('0.00', '0'),
            },
          },
        },
      ],
    });

    expect(formatConfirmedFinalReport(filled)).toEqual([
      'CONFIRMED task=ctwap-1 owner=ctwap-1 unit=slice-1 caller=caller-1 wire=wire-1 venue=venue-1 status=filled total=0.01 fill=0.01',
      'TOTAL task=ctwap-1 confirmed=0.01 rejected=0.00 residual=0.00 reserved=0.00',
    ]);
  });
});
