import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

type Credentials = { primary?: { key?: string; secret?: string }; secondary?: { key?: string; secret?: string } };
type Constructor = new (venue: string, optionsJson: string, signerSetupJson?: string) => WasmEngineLike;

/** Trusted local-provider setup. Secret strings stay inside this capsule until
 * consumed by the Rust provider; JS cannot promise physical string zeroization. */
export class LocalSignerSetup {
  #credentials: Credentials | undefined;
  readonly hasSecondary: boolean;
  readonly primaryComplete: boolean;

  constructor(options: ExchangeOptions) {
    const secondary =
      options.secondary_key !== undefined ||
      options.secondary_secret !== undefined ||
      options.secondary_passphrase !== undefined;
    if (secondary && (!options.secondary_key || !options.secondary_secret)) {
      const message = 'Rust engine secondary signing keys require a nonempty key and secret';
      throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
    }
    this.hasSecondary = secondary;
    this.primaryComplete = Boolean(options.key && options.secret);
    this.#credentials = {
      ...(options.key !== undefined || options.secret !== undefined
        ? { primary: { key: options.key, secret: options.secret } }
        : {}),
      ...(secondary ? { secondary: { key: options.secondary_key, secret: options.secondary_secret } } : {}),
    };
  }

  construct(Constructor: Constructor, venue: string, optionsJson: string): WasmEngineLike {
    const credentials = this.#credentials;
    if (!credentials) throw new Error('Local signer setup has already been consumed or disposed');
    this.#credentials = undefined;
    try {
      return new Constructor(venue, optionsJson, JSON.stringify(credentials));
    } finally {
      if (credentials.primary) {
        credentials.primary.key = undefined;
        credentials.primary.secret = undefined;
      }
      if (credentials.secondary) {
        credentials.secondary.key = undefined;
        credentials.secondary.secret = undefined;
      }
    }
  }

  dispose(): void {
    this.#credentials = undefined;
  }
}

/** Caller-owned input is unchanged; the generic exchange receives no credentials. */
export const withoutSigningCredentials = (options: ExchangeOptions): ExchangeOptions => {
  const {
    key: _key,
    secret: _secret,
    passphrase: _passphrase,
    secondary_key: _readKey,
    secondary_secret: _readSecret,
    secondary_passphrase: _readPassphrase,
    ...routing
  } = options;
  // BaseExchange's legacy type requires key/secret; Rust uses the cached
  // construction record and provider refs instead of reading those fields.
  return routing as ExchangeOptions;
};
