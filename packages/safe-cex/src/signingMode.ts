import type { ExchangeOptions } from './types';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';

/** These are the existing account-worker init wire modes; absence keeps legacy selection. */
export function validateSigningMode(mode: unknown): ExchangeOptions['signingMode'] {
  if (mode === undefined || mode === 'local' || mode === 'cosigned' || mode === 'delegated') return mode;
  const message = 'Unsupported signing mode';
  throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
}
