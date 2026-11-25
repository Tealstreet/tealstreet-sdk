import type { ExchangeAccountConfig } from '@tealstreet/cli-core';

import { exchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';

export const SIGNING_KEY_MASK = '******';

type KeyDisplayAccount = Pick<ExchangeAccountConfig, 'exchange' | 'apiKey' | 'paradexAddress'>;

/**
 * The `account list` line for an account's key. On `requiresNoSecret` venues
 * (Paradex, Polymarket) `apiKey` holds a private key, so no part of it is shown:
 * the public address if the account has one, otherwise a fixed mask. Unknown venues are treated the same way.
 */
export function formatAccountKeyLine(account: KeyDisplayAccount): string {
  const metadata = exchangeMetadata[account.exchange as keyof typeof exchangeMetadata];
  if (!metadata || metadata.requiresNoSecret) {
    const address = account.paradexAddress?.trim();
    return address ? `Address: ${address}` : `API Key: ${SIGNING_KEY_MASK}`;
  }
  return `API Key: ${maskPublicKey(account.apiKey || '')}`;
}

function maskPublicKey(key: string): string {
  if (!key) return '(not set)';
  if (key.length <= 8) return '****';
  return key.slice(0, 4) + '****' + key.slice(-4);
}
