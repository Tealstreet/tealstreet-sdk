import type { LocalIdentityMetadata, LocalSignerMetadata, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

type LocalIdentity = LocalIdentityMetadata & { key?: string; secret?: string; passphrase?: string };
type Credentials = { primary?: LocalIdentity; secondary?: LocalIdentity };
type Constructor = new (venue: string, optionsJson: string, signerSetupJson?: string) => WasmEngineLike;

const invalidMetadata = (): never => {
  const message = 'Invalid local signer setup metadata';
  throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
};
const record = (value: unknown, fields: readonly string[]): Record<string, unknown> => {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).some((key) => !fields.includes(key)))
    return invalidMetadata();
  return value as Record<string, unknown>;
};
const identityMetadata = (input: unknown): LocalIdentityMetadata | undefined => {
  if (input === undefined) return undefined;
  const value = record(input, ['stableId', 'secretEncoding', 'trimCredential']);
  const { stableId, secretEncoding, trimCredential } = value;
  if (stableId !== undefined && (typeof stableId !== 'string' || !stableId.trim() || new TextEncoder().encode(stableId).length > 256 || /[\u0000-\u001f\u007f-\u009f]/u.test(stableId)))
    return invalidMetadata();
  if (secretEncoding !== undefined && (typeof secretEncoding !== 'string' || !['utf8', 'base64', 'hex', 'pem'].includes(secretEncoding)))
    return invalidMetadata();
  if (trimCredential !== undefined && typeof trimCredential !== 'boolean') return invalidMetadata();
  return { stableId, secretEncoding, trimCredential } as LocalIdentityMetadata;
};
const signerMetadata = (input: unknown): LocalSignerMetadata => {
  if (input === undefined) return {};
  const value = record(input, ['primary', 'secondary']);
  return { primary: identityMetadata(value.primary), secondary: identityMetadata(value.secondary) };
};

/** Trusted local-provider setup. Secret strings stay inside this capsule until
 * consumed by the Rust provider; JS cannot promise physical string zeroization. */
export class LocalSignerSetup {
  #credentials: Credentials | undefined;
  readonly hasSecondary: boolean;
  readonly primaryComplete: boolean;

  constructor(options: ExchangeOptions) {
    const setupMetadata = signerMetadata(options.localSigner);
    const secondary =
      options.secondary_key !== undefined ||
      options.secondary_secret !== undefined ||
      options.secondary_passphrase !== undefined ||
      setupMetadata.secondary !== undefined;
    if (secondary && (!options.secondary_key || !options.secondary_secret)) {
      const message = 'Rust engine secondary signing keys require a nonempty key and secret';
      throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
    }
    this.hasSecondary = secondary;
    this.primaryComplete = Boolean(options.key && options.secret);
    this.#credentials = {
      ...(options.key !== undefined || options.secret !== undefined || options.passphrase !== undefined || setupMetadata.primary !== undefined
        ? { primary: { ...setupMetadata.primary, key: options.key, secret: options.secret, passphrase: options.passphrase } }
        : {}),
      ...(secondary ? { secondary: { ...setupMetadata.secondary, key: options.secondary_key, secret: options.secondary_secret, passphrase: options.secondary_passphrase } } : {}),
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
        credentials.primary.passphrase = undefined;
      }
      if (credentials.secondary) {
        credentials.secondary.key = undefined;
        credentials.secondary.secret = undefined;
        credentials.secondary.passphrase = undefined;
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
    localSigner: _setupMetadata,
    ...routing
  } = options;
  // BaseExchange's legacy type requires key/secret; Rust uses the cached
  // construction record and provider refs instead of reading those fields.
  return routing as ExchangeOptions;
};
