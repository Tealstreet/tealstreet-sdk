import type { LocalIdentityMetadata, LocalSignerMetadata, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';
import type { CredentialPresence, IdentitySetupPolicy, SignerSetupPolicy } from './construction';

type LocalIdentity = LocalIdentityMetadata & {
  key?: string;
  secret?: string;
  passphrase?: string;
  /** Secondary only, set by a family setup mapping: sign with the primary's ref. */
  signingFromPrimary?: true;
  /** Primary only, set by a family setup mapping: role-bound provider material. */
  extra?: { zklinkSeed?: { value: string; encoding: 'hex' } };
};
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
  if (secretEncoding !== undefined && (typeof secretEncoding !== 'string' || !['utf8', 'base64', 'hex', 'pem', 'base64Text', 'base58'].includes(secretEncoding)))
    return invalidMetadata();
  if (trimCredential !== undefined && typeof trimCredential !== 'boolean') return invalidMetadata();
  return { stableId, secretEncoding, trimCredential } as LocalIdentityMetadata;
};
export const parseLocalSignerMetadata = (input: unknown): LocalSignerMetadata => {
  if (input === undefined) return {};
  const value = record(input, ['primary', 'secondary']);
  return { primary: identityMetadata(value.primary), secondary: identityMetadata(value.secondary) };
};

// Match Rust str::trim's Unicode White_Space semantics. JS String.trim also
// removes BOM and keeps NEL, which would disagree with the provider's key bytes.
export const rustTrim = (value: string): string => value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '');
const credentialPresent = (key: string | undefined, trim: boolean): boolean =>
  typeof key === 'string' && (trim ? rustTrim(key) : key).length > 0;

export const effectiveIdentityMetadata = (
  metadata: LocalIdentityMetadata | undefined,
  policy: IdentitySetupPolicy | undefined,
): LocalIdentityMetadata | undefined => {
  let effective = metadata;
  if (policy?.trimCredential !== undefined) effective = { ...effective, trimCredential: policy.trimCredential };
  if (policy?.trimSigning !== undefined) effective = { ...effective, trimSigning: policy.trimSigning };
  return effective;
};

/** One required-field/fallback policy for trusted local and callback setup.
 * Local setup validates but retains the original absent input for Rust's ref
 * aliasing; callback setup uses the resolution to assign the same owned ref. */
export const resolveSetupPassphrase = (
  slot: 'primary' | 'secondary',
  complete: boolean,
  passphrase: string | undefined,
  primaryPassphrase: string | undefined,
  policy: IdentitySetupPolicy | undefined,
  publicDataOnly: boolean,
): Readonly<{ value?: string; inheritPrimary: boolean }> => {
  const rule = policy?.passphrase;
  const inheritPrimary = slot === 'secondary' && rule === 'requiredWithPrimaryFallback'
    && (passphrase === undefined || passphrase === null);
  const effective = inheritPrimary ? primaryPassphrase : passphrase;
  const required = complete && rule !== undefined && rule !== 'optional'
    && !(rule === 'requiredForPrivate' && slot === 'primary' && publicDataOnly);
  if (required && (typeof effective !== 'string' || effective.length === 0)) {
    const message = `Rust engine ${slot} signing credentials require a passphrase`;
    throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
  }
  return { value: typeof effective === 'string' ? effective : undefined, inheritPrimary };
};

/** Trusted local-provider setup. Secret strings stay inside this capsule until
 * consumed by the Rust provider; JS cannot promise physical string zeroization. */
export class LocalSignerSetup {
  #credentials: Credentials | undefined;
  readonly hasSecondary: boolean;
  readonly primaryComplete: boolean;
  readonly credentialPresence: CredentialPresence;
  /** A ValidateOnly family's read-key result (Rust `ReadCredentials::validate`); the key never leaves. */
  readonly readCredentialsValid: boolean | undefined;

  constructor(
    options: ExchangeOptions,
    policy: SignerSetupPolicy = {},
    secondarySigningFromPrimary = false,
    zklinkSeed?: string,
  ) {
    const setupMetadata = parseLocalSignerMetadata(options.localSigner);
    const primaryMetadata = effectiveIdentityMetadata(setupMetadata.primary, policy.primary);
    const secondaryMetadata = effectiveIdentityMetadata(setupMetadata.secondary, policy.secondary);
    const validateOnly = policy.readCredentials === 'validateOnly';
    if (validateOnly && setupMetadata.secondary !== undefined) invalidMetadata();
    this.readCredentialsValid = validateOnly ? readCredentialsValidity(options) : undefined;
    const secondary = !validateOnly && (
      options.secondary_key !== undefined ||
      options.secondary_secret !== undefined ||
      options.secondary_passphrase !== undefined ||
      setupMetadata.secondary !== undefined);
    if (secondary && (!credentialPresent(options.secondary_key, secondaryMetadata?.trimCredential === true)
      || (!secondarySigningFromPrimary
        && !credentialPresent(options.secondary_secret, secondaryMetadata?.trimSigning === true)))) {
      const message = 'Rust engine secondary signing keys require a nonempty key and secret';
      throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
    }
    this.hasSecondary = secondary;
    this.credentialPresence = Object.freeze({
      key: credentialPresent(options.key, primaryMetadata?.trimCredential === true),
      secret: credentialPresent(options.secret, primaryMetadata?.trimSigning === true),
    });
    this.primaryComplete = this.credentialPresence.key && this.credentialPresence.secret;
    resolveSetupPassphrase('primary', this.primaryComplete, options.passphrase, options.passphrase, policy.primary, options.publicDataOnly === true);
    resolveSetupPassphrase('secondary', secondary, options.secondary_passphrase, options.passphrase, policy.secondary, options.publicDataOnly === true);
    // A public-only PublicRead provider holds no primary (native drops it too).
    const primaryDropped = policy.readCredentials === 'publicRead' && options.publicDataOnly === true;
    this.#credentials = {
      ...(!primaryDropped && (options.key !== undefined || options.secret !== undefined || options.passphrase !== undefined || setupMetadata.primary !== undefined)
        ? { primary: {
          ...primaryMetadata, key: options.key, secret: options.secret, passphrase: options.passphrase,
          ...(zklinkSeed ? { extra: { zklinkSeed: { value: zklinkSeed, encoding: 'hex' as const } } } : {}),
        } }
        : {}),
      ...(secondary
        ? { secondary: secondarySigningFromPrimary
          ? { ...secondaryMetadata, key: options.secondary_key, signingFromPrimary: true as const }
          : { ...secondaryMetadata, key: options.secondary_key, secret: options.secondary_secret, passphrase: options.secondary_passphrase } }
        : {}),
    };
  }

  construct(Constructor: Constructor, venue: string, optionsJson: string): WasmEngineLike {
    const credentials = this.#credentials;
    if (!credentials) throw new Error('Local signer setup has already been consumed or disposed');
    this.#credentials = undefined;
    try {
      return new Constructor(venue, withReadCredentialsValid(optionsJson, this.readCredentialsValid), JSON.stringify(credentials));
    } finally {
      if (credentials.primary) {
        credentials.primary.key = undefined;
        credentials.primary.secret = undefined;
        credentials.primary.passphrase = undefined;
        credentials.primary.extra = undefined;
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

/** A ValidateOnly read key's `ReadCredentials::validate` result, without the key.
 * Presence follows the legacy bridge (a truthy `secondary_key`); validity is
 * Rust's rule: key and secret both nonempty, untrimmed. */
export const readCredentialsValidity = (options: ExchangeOptions): boolean | undefined =>
  options.secondary_key ? typeof options.secondary_secret === 'string' && options.secondary_secret.length > 0 : undefined;

/** Splice the result in as text: re-parsing could round large numeric options.
 * A caller-supplied duplicate key is refused by the Rust parser. */
const withReadCredentialsValid = (optionsJson: string, valid: boolean | undefined): string => {
  if (valid === undefined) return optionsJson;
  const open = optionsJson.indexOf('{');
  if (open < 0) return invalidMetadata();
  const rest = optionsJson.slice(open + 1);
  return `${optionsJson.slice(0, open)}{"readCredentialsValid":${valid}${/^\s*\}/.test(rest) ? '' : ','}${rest}`;
};

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
    // ApeX's OmniKey seed is signing material: it lives only in provider setup.
    omniKeySeed: _omniKeySeed,
    auth,
    ...routing
  } = options;
  // AsterDEX's wallet key is signing material; its public addresses stay.
  if (auth) {
    const { privateKey: _privateKey, ...addresses } = auth;
    return { ...routing, auth: addresses } as ExchangeOptions;
  }
  // BaseExchange's legacy type requires key/secret; Rust uses the cached
  // construction record and provider refs instead of reading those fields.
  return routing as ExchangeOptions;
};
