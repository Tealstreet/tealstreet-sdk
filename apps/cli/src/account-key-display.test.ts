import type { ExchangeName } from '@tealstreet/safe-cex/src/types';

import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';
import { formatAccountKeyLine, SIGNING_KEY_MASK } from './account-key-display';

const PRIVATE_KEY = `0x${'abcdef0123456789'.repeat(4)}`;
const ADDRESS = '0x1111222233334444555566667777888899990000';

const expectNoKeyFragment = (shown: string) => {
  for (const fragment of [PRIVATE_KEY.slice(0, 4), PRIVATE_KEY.slice(2, 6), PRIVATE_KEY.slice(-4)]) {
    expect(shown).not.toContain(fragment);
  }
};

describe('formatAccountKeyLine', () => {
  it('keeps the first and last four characters for venues whose apiKey is public', () => {
    expect(formatAccountKeyLine({ exchange: 'bybit', apiKey: 'ABCDEFGHIJKL' })).toBe('API Key: ABCD****IJKL');
    expect(formatAccountKeyLine({ exchange: 'bybit', apiKey: '' })).toBe('API Key: (not set)');
  });

  it.each(['paradex', 'polymarket'] as ExchangeName[])('never shows any part of the %s private key', (exchange) => {
    expect(exchangeMetadata[exchange].requiresNoSecret).toBe(true);
    const shown = formatAccountKeyLine({ exchange, apiKey: PRIVATE_KEY });
    expect(shown).toBe(`API Key: ${SIGNING_KEY_MASK}`);
    expectNoKeyFragment(shown);
  });

  it('shows the public address instead when the account has one', () => {
    const shown = formatAccountKeyLine({ exchange: 'paradex', apiKey: PRIVATE_KEY, paradexAddress: ADDRESS });
    expect(shown).toBe(`Address: ${ADDRESS}`);
    expectNoKeyFragment(shown);
  });

  it.each(['retiredvenue', 'constructor', '__proto__'])(
    'masks unknown venue %s rather than guessing the key is public',
    (exchange) => {
      const shown = formatAccountKeyLine({ exchange: exchange as ExchangeName, apiKey: PRIVATE_KEY });
      expect(shown).toBe(`API Key: ${SIGNING_KEY_MASK}`);
    },
  );
});
