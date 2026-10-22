import type { Signer, SigningIdentities, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';
import type { CredentialPresence } from './construction';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { nanoid } from 'nanoid';

import { parseLocalSignerMetadata, withoutSigningCredentials } from './localSignerSetup';
import { createRemoteSignerProvider } from './remoteSignerProvider';

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

/** Trusted setup for aliases admitted by the shared caller policy. This capsule
 * does not enable a venue or retain local signing material. Remote authority
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
  readonly hasSecondary: boolean;
  readonly primaryComplete: boolean;
  readonly credentialPresence: CredentialPresence;

  constructor(name: ExchangeName, options: ExchangeOptions) {
    const metadata = parseLocalSignerMetadata(options.localSigner);
    this.intent = Object.freeze({ remoteSigner: options.remoteSigner !== undefined, payloadSigner: options.payloadSigner !== undefined });
    const remoteSigner = options.remoteSigner;
    const payloadSigner = remoteSigner ? undefined : options.payloadSigner;
    if (!remoteSigner && !payloadSigner) invalid('Delegated signing requires a configured callback');
    if ((this.intent.remoteSigner && (!remoteSigner || typeof remoteSigner.sign !== 'function'
      || typeof remoteSigner.isAvailable !== 'function' || typeof remoteSigner.onAvailabilityChange !== 'function'))
      || (this.intent.payloadSigner && typeof options.payloadSigner !== 'function')) invalid();
    this.#publicMode = Object.freeze({ requested: options.publicDataOnly ?? null,
      ...this.intent, apiKeyIndexPresent: Boolean(options.apiKeyIndex) });
    const primary = publicCredential(options.key, metadata.primary?.trimCredential === true);
    const secondary = publicCredential(options.secondary_key, metadata.secondary?.trimCredential === true);
    const secondaryRequested = options.secondary_key !== undefined || options.secondary_secret !== undefined
      || options.secondary_passphrase !== undefined || metadata.secondary !== undefined;
    if (secondaryRequested && !secondary) invalid('Rust engine secondary delegated signing requires a nonempty public key');
    const identity = (stableId: string | undefined) => Object.freeze({
      credential: `delegated/${nanoid()}/credential`,
      signing: `delegated/${nanoid()}/signing`,
      ...(stableId === undefined ? {} : { stableId }),
    });
    const identities: SigningIdentities = Object.freeze({
      ...(primary === undefined ? {} : { primary: identity(metadata.primary?.stableId) }),
      ...(secondary === undefined ? {} : { secondary: identity(metadata.secondary?.stableId) }),
    });
    try {
      this.#provider = createRemoteSignerProvider({
        exchangeName: name,
        identities: {
          ...(identities.primary && primary !== undefined ? { primary: { ...identities.primary, publicCredential: primary } } : {}),
          ...(identities.secondary && secondary !== undefined ? { secondary: { ...identities.secondary, publicCredential: secondary } } : {}),
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
        remoteSigning: true, payloadSigner: this.intent.payloadSigner, publicMode: this.#publicMode }));
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
