import type { SignAlgorithm, Signer, SigningIdentities, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';
import type { CredentialPresence } from './construction';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { nanoid } from 'nanoid';

import { derivePublicMode, deriveRemoteSigningPolicy, deriveSignerSetupPolicy } from './construction';
import { effectiveIdentityMetadata, parseLocalSignerMetadata, resolveSetupPassphrase, withoutSigningCredentials } from './localSignerSetup';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from './remoteSignerProvider';

type Constructor = new (venue: string, optionsJson: string, signerSetupJson?: string) => WasmEngineLike;
const invalid = (message = 'Invalid delegated signer setup'): never => {
  throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
};
const publicCredential = (input: unknown, trim: boolean): string | undefined => {
  if (input === undefined) return undefined;
  if (typeof input !== 'string') return invalid();
  const key = trim ? input.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '') : input;
  return key.length ? key : undefined;
};

/** Presence selects a mode; malformed configured callbacks still fail validation. */
export const selectDelegatedSigningMode = (
  options: Pick<ExchangeOptions, 'remoteSigner' | 'payloadSigner'>,
): 'remote' | 'payload' | undefined => options.remoteSigner !== undefined ? 'remote'
  : options.payloadSigner !== undefined ? 'payload' : undefined;

/** Trusted setup for aliases admitted by the shared caller policy. This capsule
 * does not enable a venue or retain a local signing secret. Remote authority
 * precedes PayloadSigner; neither can fall back to a local secret.
 */
export class DelegatedSignerSetup {
  #provider: Signer | undefined;
  #identities: SigningIdentities | undefined;
  #consumed = false;
  #abort = new AbortController();
  #pending = new Set<AbortController>();
  #venue: ExchangeName;
  #publicMode: Readonly<{ requested: boolean | null; remoteSigner: boolean; payloadSigner: boolean; apiKeyIndexPresent: boolean }>;
  readonly intent: Readonly<{ remoteSigner: boolean; payloadSigner: boolean }>;
  readonly usesPayloadSigner: boolean;
  readonly hasSecondary: boolean;
  readonly primaryComplete: boolean;
  readonly credentialPresence: CredentialPresence;
  readonly signerAlgorithms: readonly SignAlgorithm[];

  constructor(name: ExchangeName, options: ExchangeOptions) {
    this.signerAlgorithms = remoteSignerAlgorithms(name);
    const metadata = parseLocalSignerMetadata(options.localSigner);
    const setupPolicy = deriveSignerSetupPolicy(name);
    const primaryMetadata = effectiveIdentityMetadata(metadata.primary, setupPolicy.primary);
    const secondaryMetadata = effectiveIdentityMetadata(metadata.secondary, setupPolicy.secondary);
    this.intent = Object.freeze({ remoteSigner: options.remoteSigner !== undefined, payloadSigner: options.payloadSigner !== undefined });
    this.usesPayloadSigner = selectDelegatedSigningMode(options) === 'payload';
    const remoteSigner = options.remoteSigner;
    const payloadSigner = this.usesPayloadSigner ? options.payloadSigner : undefined;
    if (!remoteSigner && !payloadSigner) invalid('Delegated signing requires a configured callback');
    if ((this.intent.remoteSigner && (!remoteSigner || typeof remoteSigner.sign !== 'function'
      || typeof remoteSigner.isAvailable !== 'function' || typeof remoteSigner.onAvailabilityChange !== 'function'))
      || (this.intent.payloadSigner && typeof options.payloadSigner !== 'function')) invalid();
    this.#publicMode = Object.freeze({ requested: options.publicDataOnly ?? null,
      ...this.intent, apiKeyIndexPresent: Boolean(options.apiKeyIndex) });
    const primary = publicCredential(options.key, primaryMetadata?.trimCredential === true);
    const secondary = publicCredential(options.secondary_key, secondaryMetadata?.trimCredential === true);
    const secondaryRequested = options.secondary_key !== undefined || options.secondary_secret !== undefined
      || options.secondary_passphrase !== undefined || metadata.secondary !== undefined;
    if (secondaryRequested && !secondary) invalid('Rust engine secondary delegated signing requires a nonempty public key');
    const presence = { key: primary !== undefined, secret: Boolean(remoteSigner || payloadSigner) };
    const publicDataOnly = derivePublicMode(name, options, undefined, presence).publicDataOnly;
    const primaryPassphrase = resolveSetupPassphrase('primary', primary !== undefined, options.passphrase,
      options.passphrase, setupPolicy.primary, publicDataOnly);
    const secondaryPassphrase = resolveSetupPassphrase('secondary', secondary !== undefined, options.secondary_passphrase,
      options.passphrase, setupPolicy.secondary, publicDataOnly);
    const presentsPassphrase = deriveRemoteSigningPolicy(name)?.passphraseHeader !== undefined;
    const primaryPassphraseRef = presentsPassphrase && primaryPassphrase.value
      ? `delegated/${nanoid()}/passphrase` : undefined;
    const secondaryPassphraseRef = !presentsPassphrase || secondary === undefined || !secondaryPassphrase.value ? undefined
      : secondaryPassphrase.inheritPrimary ? primaryPassphraseRef : `delegated/${nanoid()}/passphrase`;
    const identity = (stableId: string | undefined, passphrase: string | undefined) => Object.freeze({
      credential: `delegated/${nanoid()}/credential`,
      signing: `delegated/${nanoid()}/signing`,
      ...(stableId === undefined ? {} : { stableId }),
      ...(passphrase === undefined ? {} : { passphrase }),
    });
    const identities: SigningIdentities = Object.freeze({
      ...(primary === undefined ? {} : { primary: identity(primaryMetadata?.stableId, primaryPassphraseRef) }),
      ...(secondary === undefined ? {} : { secondary: identity(secondaryMetadata?.stableId, secondaryPassphraseRef) }),
    });
    try {
      this.#provider = createRemoteSignerProvider({
        exchangeName: name,
        identities: {
          ...(identities.primary && primary !== undefined ? { primary: { ...identities.primary, publicCredential: primary,
            ...(primaryPassphraseRef === undefined ? {} : { passphraseValue: primaryPassphrase.value }) } } : {}),
          ...(identities.secondary && secondary !== undefined ? { secondary: { ...identities.secondary, publicCredential: secondary,
            ...(secondaryPassphraseRef === undefined ? {} : { passphraseValue: secondaryPassphrase.value }) } } : {}),
        },
        remoteSigner,
        payloadSigner,
        payloadSignerMetadata: options,
      });
    } catch { invalid(); }
    this.#venue = name;
    this.#identities = identities;
    this.hasSecondary = secondary !== undefined;
    this.primaryComplete = primary !== undefined;
    this.credentialPresence = Object.freeze({ key: this.primaryComplete, secret: Boolean(remoteSigner || payloadSigner) });
  }

  /** This stable wrapper owns no callbacks itself. A host may retain it after
   * disposal without retaining authority or admitting another provider call. */
  readonly signer: Signer = async (request, { signal }) => {
    const provider = this.#provider;
    if (!provider || this.#abort.signal.aborted || signal.aborted) return invalid('Delegated signer setup is disposed or canceled');
    if (this.#pending.size >= 64) return invalid('Delegated signer request limit exceeded');
    const attempt = new AbortController();
    const abort = () => attempt.abort();
    this.#pending.add(attempt);
    signal.addEventListener('abort', abort, { once: true });
    try {
      if (signal.aborted || this.#abort.signal.aborted) abort();
      return await provider(request, { signal: attempt.signal });
    } finally {
      this.#pending.delete(attempt);
      signal.removeEventListener('abort', abort);
    }
  };

  construct(Constructor: Constructor, venue: string, optionsJson: string): WasmEngineLike {
    if (this.#consumed || !this.#provider || !this.#identities) invalid('Delegated signer setup has already been consumed or disposed');
    this.#consumed = true;
    try {
      if (venue !== this.#venue) invalid();
      let parsed: unknown;
      try { parsed = JSON.parse(optionsJson); } catch { invalid(); }
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) invalid();
      const {
        remoteSigner: _remote, payloadSigner: _payload, signing: _signing, readCredentials: _readCredentials, ...safe
      } = withoutSigningCredentials(parsed as ExchangeOptions) as ExchangeOptions & { signing?: unknown; readCredentials?: unknown };
      return new Constructor(venue, JSON.stringify({ ...safe, signing: this.#identities,
        remoteSigning: true, payloadSigner: this.usesPayloadSigner, publicMode: this.#publicMode }));
    } catch (error) {
      this.dispose();
      throw error;
    }
  }

  dispose(): void {
    this.#provider = undefined;
    this.#identities = undefined;
    this.#abort.abort();
    for (const attempt of this.#pending) attempt.abort();
    this.#pending.clear();
  }
}

/** Only admitted delegated setups remove callbacks; legacy families retain theirs. */
export const withoutDelegatedSigning = (options: ExchangeOptions): ExchangeOptions => {
  const { remoteSigner: _remote, payloadSigner: _payload, ...safe } = withoutSigningCredentials(options);
  return safe as ExchangeOptions;
};
