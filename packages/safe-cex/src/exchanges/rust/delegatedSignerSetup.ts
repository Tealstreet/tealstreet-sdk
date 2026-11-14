import type { SignAlgorithm, Signer, SigningIdentities, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';
import type { CredentialPresence } from './construction';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { nanoid } from 'nanoid';

import { resolveAsterDEXSignerVersion } from './asterdexSignerSetup';
import { derivePublicMode, deriveRemoteSigningPolicy, deriveSignerSetupPolicy } from './construction';
import { effectiveIdentityMetadata, parseLocalSignerMetadata, resolveEffectivePublicDataOnly, resolveSetupPassphrase, withoutSigningCredentials } from './localSignerSetup';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from './remoteSignerProvider';
import { capturedDelegatedCallbacks, snapshotDelegatedSigningOptions } from './delegatedSigningSnapshot';

export { delegatedCallbacksWellFormed } from './delegatedSigningSnapshot';

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

/** Presence selects a mode; private setup validates malformed configured callbacks. */
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
  #coinbaseSigning: { keyName: string; algorithm: 'es256' | 'edDsa' } | undefined;
  #abort = new AbortController();
  #pending = new Set<AbortController>();
  #venue: ExchangeName;
  readonly #publicOnly: boolean;
  #publicMode: Readonly<{ requested: boolean | null; remoteSigner: boolean; payloadSigner: boolean; apiKeyIndexPresent: boolean }>;
  readonly intent: Readonly<{ remoteSigner: boolean; payloadSigner: boolean }>;
  readonly usesPayloadSigner: boolean;
  readonly hasSecondary: boolean;
  readonly primaryComplete: boolean;
  readonly credentialPresence: CredentialPresence;
  readonly signerAlgorithms: readonly SignAlgorithm[];

  constructor(name: ExchangeName, options: ExchangeOptions) {
    options = snapshotDelegatedSigningOptions(name, options);
    this.#venue = name;
    this.#publicOnly = resolveEffectivePublicDataOnly(name, options);
    this.intent = Object.freeze({ remoteSigner: options.remoteSigner !== undefined, payloadSigner: options.payloadSigner !== undefined });
    this.#publicMode = Object.freeze({ requested: options.publicDataOnly ?? null,
      ...this.intent, apiKeyIndexPresent: Boolean(options.apiKeyIndex) });
    this.usesPayloadSigner = !this.#publicOnly && selectDelegatedSigningMode(options) === 'payload';
    this.signerAlgorithms = this.#publicOnly ? [] : remoteSignerAlgorithms(name);
    // Effective public mode grants no signing authority. Do not validate or retain
    // callbacks, keys, passphrases or setup metadata that this account cannot use.
    if (this.#publicOnly) {
      this.#identities = Object.freeze({});
      this.hasSecondary = false;
      this.primaryComplete = false;
      this.credentialPresence = Object.freeze({ key: false, secret: false });
      return;
    }
    const metadata = parseLocalSignerMetadata(options.localSigner);
    const setupPolicy = deriveSignerSetupPolicy(name);
    const primaryMetadata = effectiveIdentityMetadata(metadata.primary, setupPolicy.primary);
    const secondaryMetadata = effectiveIdentityMetadata(metadata.secondary, setupPolicy.secondary);
    const apexomni = name === 'apexomni';
    if (apexomni && this.usesPayloadSigner) {
      const message = 'ApeX Omni requires RemoteSigner HMAC and zkLink contract custody';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    if (name === 'kraken' && this.usesPayloadSigner) {
      const message = 'Kraken Futures requires RemoteSigner SHA256/HMAC-SHA512 custody; PayloadSigner is unsupported';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    if (name === 'grvt' && this.usesPayloadSigner) {
      const message = 'GRVT PayloadSigner is unsupported';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    const coinbase = name === 'coinbase' || name === 'coinbasespot';
    if (coinbase && options.publicDataOnly !== true) {
      if (typeof options.key !== 'string' || !/^organizations\/[^/\s]+\/apiKeys\/[^/\s]+$/.test(options.key)) invalid('Coinbase requires a CDP API key name');
      if (options.coinbaseJwtAlgorithm !== undefined && !['es256', 'edDsa'].includes(options.coinbaseJwtAlgorithm)) invalid('Coinbase unsupported JWT algorithm');
      this.#coinbaseSigning = { keyName: options.key, algorithm: options.coinbaseJwtAlgorithm ?? 'es256' };
    }
    const kucoin = name === 'kucoin' || name === 'kucoinspot';
    if (kucoin && this.usesPayloadSigner) {
      const message = 'KuCoin requires RemoteSigner batch custody for request, passphrase and optional broker MACs';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    const callbacks = capturedDelegatedCallbacks(options) ?? invalid();
    const remoteSigner = callbacks.remoteSigner;
    const payloadSigner = this.usesPayloadSigner ? callbacks.payloadSigner : undefined;
    if (!remoteSigner && !payloadSigner) invalid('Delegated signing requires a configured callback');
    const primary = publicCredential(options.key, primaryMetadata?.trimCredential === true);
    const secondary = publicCredential(options.secondary_key, secondaryMetadata?.trimCredential === true);
    const secondaryRequested = options.secondary_key !== undefined || options.secondary_secret !== undefined
      || options.secondary_passphrase !== undefined || metadata.secondary !== undefined;
    if (apexomni && secondaryRequested) {
      const message = 'ApeX Omni secondary read keys are unsupported';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    if (coinbase && secondaryRequested) {
      const message = 'Coinbase Hosted secondary signing is unsupported; remove the secondary credentials';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    // Raw HMAC callbacks have no slot selector; never install unusable read refs.
    if ((name === 'woo' || name === 'bitrue' || name === 'cryptocom' || name === 'mexc') && remoteSigner && secondaryRequested) {
      const venue = name === 'woo' ? 'WOO X' : name === 'bitrue' ? 'Bitrue' : name === 'mexc' ? 'MEXC' : 'Crypto.com';
      const message = `${venue} secondary read keys are unsupported on the Hosted Server; remove the secondary credentials or turn Hosted Server off to use them in this browser`;
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    if (secondaryRequested && !secondary) invalid('Rust engine secondary delegated signing requires a nonempty public key');
    const presence = { key: primary !== undefined, secret: Boolean(remoteSigner || payloadSigner) };
    const publicDataOnly = derivePublicMode(name, options, undefined, presence).publicDataOnly;
    if (name === 'asterdex' && this.usesPayloadSigner && !publicDataOnly && resolveAsterDEXSignerVersion(options) === 'v3') {
      const message = 'AsterDEX V3 requires RemoteSigner API-wallet custody; PayloadSigner supports V1 HMAC only';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    if (name === 'backpack' && this.usesPayloadSigner && !publicDataOnly) {
      const message = 'Backpack requires RemoteSigner Ed25519 custody';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    const primaryDropped = setupPolicy.readCredentials === 'publicRead' && publicDataOnly;
    const primaryPassphrase = kucoin ? { value: undefined, inheritPrimary: false } : resolveSetupPassphrase('primary', primary !== undefined, options.passphrase,
      options.passphrase, setupPolicy.primary, publicDataOnly);
    const secondaryPassphrase = kucoin ? { value: undefined, inheritPrimary: false } : resolveSetupPassphrase('secondary', secondary !== undefined, options.secondary_passphrase,
      options.passphrase, setupPolicy.secondary, publicDataOnly);
    const presentsPassphrase = kucoin || deriveRemoteSigningPolicy(name)?.passphraseHeader !== undefined;
    const primaryPassphraseRef = kucoin && primary !== undefined ? `delegated/${nanoid()}/passphrase` : presentsPassphrase && primaryPassphrase.value
      ? `delegated/${nanoid()}/passphrase` : undefined;
    const secondaryPassphraseRef = kucoin && secondary !== undefined ? `delegated/${nanoid()}/passphrase` : !presentsPassphrase || secondary === undefined || !secondaryPassphrase.value ? undefined
      : secondaryPassphrase.inheritPrimary ? primaryPassphraseRef : `delegated/${nanoid()}/passphrase`;
    const broker = kucoin && options.extra?.kucoinBroker;
    const brokerRef = broker && broker.partner.trim() && broker.brokerName.trim() ? `delegated/${nanoid()}/broker` : undefined;
    const zklinkSeed = apexomni && primary !== undefined ? `delegated/${nanoid()}/zklink` : undefined;
    const identity = (stableId: string | undefined, passphrase: string | undefined) => Object.freeze({
      credential: `delegated/${nanoid()}/credential`,
      signing: `delegated/${nanoid()}/signing`,
      ...(stableId === undefined ? {} : { stableId }),
      ...(passphrase === undefined ? {} : { passphrase }),
      ...(brokerRef === undefined && zklinkSeed === undefined ? {} : { extra: {
        ...(brokerRef === undefined ? {} : { kucoinBroker: brokerRef }),
        ...(zklinkSeed === undefined ? {} : { zklinkSeed }),
      } }),
    });
    const identities: SigningIdentities = Object.freeze({
      ...(primary === undefined || primaryDropped ? {} : { primary: identity(primaryMetadata?.stableId, primaryPassphraseRef) }),
      ...(secondary === undefined ? {} : { secondary: identity(secondaryMetadata?.stableId, secondaryPassphraseRef) }),
    });
    try {
      this.#provider = createRemoteSignerProvider({
        exchangeName: name,
        identities: {
          ...(identities.primary && primary !== undefined ? { primary: { ...identities.primary, publicCredential: primary,
            ...(primaryPassphraseRef === undefined || kucoin ? {} : { passphraseValue: primaryPassphrase.value }) } } : {}),
          ...(identities.secondary && secondary !== undefined ? { secondary: { ...identities.secondary, publicCredential: secondary,
            ...(secondaryPassphraseRef === undefined || kucoin ? {} : { passphraseValue: secondaryPassphrase.value }) } } : {}),
        },
        remoteSigner,
        payloadSigner,
        payloadSignerMetadata: options,
      });
    } catch { invalid(); }
    this.#identities = identities;
    this.hasSecondary = secondary !== undefined;
    this.primaryComplete = primary !== undefined;
    this.credentialPresence = Object.freeze({ key: this.primaryComplete, secret: Boolean(remoteSigner || payloadSigner) });
  }

  /** This stable wrapper owns no callbacks itself. A host may retain it after
   * disposal without retaining authority or admitting another provider call. */
  readonly signer: Signer = async (request, { signal }) => {
    if (this.#publicOnly) {
      const message = 'Public-only accounts cannot sign';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
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
    if (this.#consumed || (!this.#publicOnly && !this.#provider) || !this.#identities) invalid('Delegated signer setup has already been consumed or disposed');
    this.#consumed = true;
    try {
      if (venue !== this.#venue) invalid();
      let parsed: unknown;
      try { parsed = JSON.parse(optionsJson); } catch { invalid(); }
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) invalid();
      const {
        remoteSigner: _remote, payloadSigner: _payload, signing: _signing, readCredentials: _readCredentials, ...safe
      } = withoutSigningCredentials(parsed as ExchangeOptions) as ExchangeOptions & { signing?: unknown; readCredentials?: unknown };
      return new Constructor(venue, JSON.stringify({ ...safe, ...(this.#publicOnly ? { publicDataOnly: true } : {}), signing: this.#identities,
        ...(this.#coinbaseSigning && { coinbaseSigning: this.#coinbaseSigning }),
        remoteSigning: !this.#publicOnly, payloadSigner: this.usesPayloadSigner, publicMode: this.#publicMode }));
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
