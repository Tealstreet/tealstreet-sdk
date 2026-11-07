import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

/** The account key is provider material; the Paradex and subkey addresses stay
 * public. Key decoding and Ethereum derivation belong to Rust trusted setup. */
export const mapParadexSignerSetup = (options: ExchangeOptions): ExchangeOptions => {
  if (options.secondary_key !== undefined || options.secondary_secret !== undefined
    || options.secondary_passphrase !== undefined || options.localSigner !== undefined) {
    const message = 'Invalid Paradex local signer setup';
    throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
  }
  // Native Paradex never read a secret or passphrase; they stay ignored.
  const { key, secret: _ignoredSecret, passphrase: _ignoredPassphrase, ...setup } = options;
  return { ...setup, secret: key } as ExchangeOptions;
};
