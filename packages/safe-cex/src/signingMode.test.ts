import type { ExchangeAccountConfig, ExchangeOptions } from './types';

import { describe, expect, it, vi } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { accountToExchangeOptions } from './createExchangeFromAccount';

vi.mock('./index', () => ({ createExchange: vi.fn() }));

const account: ExchangeAccountConfig = { exchange: 'bybit', name: 'main', apiKey: '', apiSecret: '' };

describe('saved account signing mode', () => {
  it.each(['local', 'cosigned', 'delegated'] as const)('propagates the existing %s wire mode', (signingMode) => {
    expect(accountToExchangeOptions({ ...account, signingMode }).signingMode).toBe(signingMode);
  });

  it('keeps omission for existing accounts and preserves explicit overrides', () => {
    expect(accountToExchangeOptions(account)).not.toHaveProperty('signingMode');
    expect(
      accountToExchangeOptions({ ...account, signingMode: 'delegated' }, { signingMode: 'cosigned' }).signingMode,
    ).toBe('cosigned');
  });

  it.each(['', 'DELEGATED', 'hosted', null, 7, {}, false])(
    'refuses unknown mode %s in accounts and overrides',
    (mode) => {
      const signingMode = mode as ExchangeOptions['signingMode'];
      for (const convert of [
        () => accountToExchangeOptions({ ...account, signingMode }),
        () => accountToExchangeOptions(account, { signingMode }),
        () => accountToExchangeOptions({ ...account, signingMode }, { signingMode: 'local' }),
      ]) {
        expect(convert).toThrowError(ExchangeError);
        expect(convert).toThrowError(expect.objectContaining({ kind: 'invalidInput', outcome: 'notSent' }));
      }
    },
  );
});
