import createHash from 'create-hash';
import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';
import { parseLocalSignerMetadata } from './localSignerSetup';

/** Preserve MEXC's persisted account namespace before the provider trims its key. */
export const mapMEXCSignerSetup = (options: ExchangeOptions): ExchangeOptions => {
  const metadata = parseLocalSignerMetadata(options.localSigner);
  if (typeof options.key !== 'string' || options.key.length === 0) return options;
  const stableId = `mexc:${createHash('sha256').update(options.key).digest('hex').slice(0, 24)}`;
  if (metadata.primary?.stableId !== undefined && metadata.primary.stableId !== stableId) {
    const message = 'MEXC primary account identity mapping conflicts with its credential';
    throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
  }
  return {
    ...options,
    localSigner: { ...metadata, primary: { ...metadata.primary, stableId } },
  };
};
