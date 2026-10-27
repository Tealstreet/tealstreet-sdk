import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

/** Public account addresses are not provider credentials. Scalar decoding belongs to Rust. */
export const mapHyperliquidSignerSetup = (options: ExchangeOptions): ExchangeOptions => {
  if (options.secondary_key !== undefined
    || options.secondary_secret !== undefined || options.secondary_passphrase !== undefined
    || options.localSigner?.secondary !== undefined) {
    const message = 'Invalid Hyperliquid local signer setup';
    throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
  }
  const { key: _accountAddress, passphrase: _ignoredLegacyPassphrase, ...setup } = options;
  return {
    ...setup,
    localSigner: { primary: { ...options.localSigner?.primary, secretEncoding: 'hex' } },
  } as ExchangeOptions;
};
