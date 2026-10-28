import { describe, expect, it } from 'vitest';

import { ExchangeHealthMonitor } from '../../../packages/safe-cex/src/health/ExchangeHealthMonitor';
import { classifyForCLI } from './error-classifier';

const BYBIT_ENV_MISMATCH_MESSAGE =
  'Bybit API key environment mismatch: this key is valid for Bybit Demo Trading, but this account is configured for live Bybit. Create a live Bybit API key or switch the account to Bybit Demo Trading. [HTTP 403 Forbidden — API key rejected]';

describe('classifyForCLI', () => {
  it('shows an insufficient-balance refusal as an order error', () => {
    expect(classifyForCLI('insufficient_balance', 'error')).toEqual({ action: 'error' });
  });

  it('keeps the generic message for ordinary invalid auth', () => {
    expect(classifyForCLI('auth_invalid', 'error')).toEqual({
      action: 'critical',
      message: 'Invalid API key or secret',
    });
  });

  it('preserves actionable Bybit environment mismatch auth text', () => {
    expect(classifyForCLI('auth_invalid', 'error', BYBIT_ENV_MISMATCH_MESSAGE)).toEqual({
      action: 'critical',
    });
  });

  it('still classifies the prepended Bybit mismatch as auth_invalid', () => {
    const monitor = new ExchangeHealthMonitor('bybit');

    expect(monitor.processMessage(BYBIT_ENV_MISMATCH_MESSAGE, true)).toBe('auth_invalid');

    monitor.dispose();
  });
});
