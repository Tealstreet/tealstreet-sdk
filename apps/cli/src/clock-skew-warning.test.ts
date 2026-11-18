import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';

import { describe, expect, it, vi } from 'vitest';

import { ExchangeEmitter } from '../../../packages/safe-cex/src/exchanges/exchangeEmitter';
import { createClockSkewWarning } from './clock-skew-warning';

const corrected = (offsetMs: number): EngineEvent => ({
  level: 'info',
  code: 'bybit.clock.corrected',
  message: 'untrusted text must not reach the advice',
  fields: { offsetMs },
});

describe('CLI clock correction warning', () => {
  it('prints one actionable Windows line across retries and accounts', () => {
    const warn = vi.fn();
    const handler = createClockSkewWarning(warn, 'win32');
    const first = new ExchangeEmitter('bybit');
    const second = new ExchangeEmitter('bybitdemo');
    for (const source of [first, second]) source.on('clock-advice', handler);
    first.emit('clock-advice', corrected(-713_451));
    first.emit('clock-advice', corrected(-713_451));
    second.emit('clock-advice', corrected(-713_451));
    expect(warn.mock.calls).toEqual([
      [
        'Your system clock is 11.9 min fast; sync your system time (Windows: Settings → Time & language → Date & time → Sync now).',
      ],
    ]);
  });

  it('names a slow clock accurately without Windows instructions on other platforms', () => {
    const warn = vi.fn();
    createClockSkewWarning(warn, 'darwin')(corrected(713_451));
    expect(warn).toHaveBeenCalledWith('Your system clock is 11.9 min slow; sync your system time.');
  });

  it('ignores ordinary jitter, unknown events and malformed evidence without spending the warning', () => {
    const warn = vi.fn();
    const handler = createClockSkewWarning(warn, 'linux');
    for (const value of [0, 999, -999, NaN, Infinity, -Infinity]) handler(corrected(value));
    handler({ ...corrected(-713_451), code: 'request.failed' });
    handler({ ...corrected(-713_451), fields: { offsetMs: '-713451' } });
    handler({ ...corrected(-713_451), fields: {} });
    expect(warn).not.toHaveBeenCalled();
    handler(corrected(-713_451));
    expect(warn).toHaveBeenCalledOnce();
  });
});
