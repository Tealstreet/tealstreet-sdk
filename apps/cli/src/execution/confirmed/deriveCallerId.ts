import { createHash } from 'crypto';

import type { ExchangeName } from '@tealstreet/safe-cex/src/types';

const DERIVATION_VERSION = 'tealstreet-client-order-id/v2-session';

/** Caller-ID character budgets after #27 broker prefixes. */
const CALLER_ID_BUDGET: Partial<Record<ExchangeName, number>> = {
  binance: 26,
  bybit: 36,
  okx: 16,
  bitget: 32,
  blofin: 16,
};

export const deriveCallerId = (input: {
  readonly venue: ExchangeName;
  readonly signerFingerprint: string;
  readonly ownerId: string;
  readonly unitId: string;
}): string => {
  if (input.signerFingerprint.length === 0 || input.ownerId.length === 0 || input.unitId.length === 0) {
    throw new Error('Caller ID derivation requires nonempty signer, owner, and unit IDs');
  }
  const budget = CALLER_ID_BUDGET[input.venue];
  if (budget === undefined) {
    throw new Error(`No session caller-ID budget for venue ${input.venue}`);
  }
  const canonical = JSON.stringify([
    DERIVATION_VERSION,
    input.venue,
    input.signerFingerprint,
    input.ownerId,
    input.unitId,
  ]);
  return createHash('sha256').update(canonical, 'utf8').digest('hex').slice(0, budget);
};
