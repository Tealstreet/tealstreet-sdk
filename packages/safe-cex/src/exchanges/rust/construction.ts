import type { EngineBridgeConstruction, EngineConstruction } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';

import rules from './construction-rules.json';
import { hasAsterDEXV3AuthContext, resolveAsterDEXApiVersion } from '../asterdex/asterdex.types';

export type PassphraseSetupPolicy = 'optional' | 'requiredForPrivate' | 'requiredWithPrimaryFallback';
export type KeyEncoding = 'utf8' | 'base64' | 'hex' | 'pem' | 'base64Text' | 'base58';
export type ExtraKeyPolicy = { key: 'zklinkSeed' | 'l1Wallet'; field?: 'optional' | 'requiredForPrivate'; encoding?: KeyEncoding };
/** Asymmetric only: a declared signing ref may answer public operations (P4). */
export type SigningAlgorithm = 'ed25519' | 'es256' | 'secp256k1Digest' | 'starkDigest';
export type IdentitySetupPolicy = {
  trimCredential?: boolean; passphrase?: PassphraseSetupPolicy; secretEncoding?: KeyEncoding; extra?: ExtraKeyPolicy;
  signingAlgorithm?: SigningAlgorithm;
};
export type CredentialSetupPolicy = { primary?: IdentitySetupPolicy; secondary?: IdentitySetupPolicy };
export type CredentialPresence = Readonly<{ key: boolean; secret: boolean }>;
export type RemoteSigningPolicy = Readonly<{
  enabled?: boolean;
  payloadSupported?: boolean;
  payloadRestOnly?: boolean;
  protectedHeaders?: readonly string[];
  signatureEncoding?: 'hex' | 'base64';
  passphraseHeader?: string;
  wire?: Readonly<{
    exchangeName: ExchangeName; extraData?: 'bybitExpiry' | 'binanceSpot' | 'secondsTimestamp'; requestId?: string;
    /** Signed literal after a seconds login stamp; required by `secondsTimestamp`. */
    secondsSuffix?: Readonly<{ method: string; path: string }>;
  }>;
}>;

type Rule = {
  canSign?: string[][]; privateEligible?: string[][]; explicitPrivateError?: string;
  signerSetup?: CredentialSetupPolicy;
  remoteSigning?: RemoteSigningPolicy;
  /** Family-specific trusted setup mapping, declared rather than name-matched. */
  setupMapping?: 'asterdex';
  bridge?: Partial<Omit<EngineBridgeConstruction, 'demo'>> & { demo?: 'always' | 'hyroMode' };
};

/** Trusted setup policy; unspecified fields retain explicit caller metadata. */
export const deriveSignerSetupPolicy = (name: ExchangeName): CredentialSetupPolicy =>
  (rules.venues as Record<string, Rule>)[name]?.signerSetup ?? {};

export const deriveSignerSetupMapping = (name: ExchangeName): Rule['setupMapping'] =>
  (rules.venues as Record<string, Rule>)[name]?.setupMapping;

export const deriveRemoteSigningPolicy = (name: ExchangeName): RemoteSigningPolicy | undefined =>
  (rules.venues as Record<string, Rule>)[name]?.remoteSigning;

/** Construction-time policy from the same declarations the native registry reports. */
export const deriveBridgeConstruction = (name: ExchangeName, options: ExchangeOptions): EngineBridgeConstruction => {
  const policy = (rules.venues as Record<string, Rule>)[name]?.bridge;
  return {
    spot: policy?.spot === true,
    demo: policy?.demo === 'always' || (policy?.demo === 'hyroMode' && options.extra?.hyrotraderMode === 'demo'),
    primaryCredentialsOnly: policy?.primaryCredentialsOnly === true,
    payloadSigning: policy?.payloadSigning === true,
    apiKeyIndexRequired: policy?.apiKeyIndexRequired === true,
    isolatedBookSubscriptions: policy?.isolatedBookSubscriptions === true,
    dispatchSigning:policy?.dispatchSigning===true,
  };
};

/** The same declarative conditions native construction reads, before async WASM loading. */
export const derivePublicMode = (
  name: ExchangeName,
  options: ExchangeOptions,
  nativeVersion?: 'v1' | 'v3',
  credentialPresence?: CredentialPresence,
): Required<Pick<EngineConstruction, 'canSign' | 'publicDataOnly'>> => {
  const rule = (rules.venues as Record<string, Rule>)[name];
  const inputs: Record<string, boolean> = {
    key: credentialPresence?.key ?? Boolean(options.key),
    secret: credentialPresence?.secret ?? Boolean(options.secret),
    remoteSigner: Boolean(options.remoteSigner),
    payloadSigner: Boolean(options.payloadSigner),
    applicationId: Boolean(options.applicationId),
    vault: Boolean(options.vault),
    paradexAddress: Boolean(options.paradexAddress),
    subkeyAddress: Boolean(options.subkeyAddress),
    apiKeyIndex: Boolean(options.apiKeyIndex),
  };
  const present = (input: string) => {
    // Aster's version/auth grammar stays under the compiled TS/native parity matrix.
    if (input === 'asterLocalAuth') return (nativeVersion ?? resolveAsterDEXApiVersion(options)) === 'v3'
      ? hasAsterDEXV3AuthContext(options) : Boolean(options.secret);
    return inputs[input];
  };
  const matches = (conditions: string[][]) => conditions.every((group) => group.some(present));
  const canSign = matches(rule?.canSign ?? rules.default.canSign);
  inputs.canSign = canSign;
  return {
    canSign,
    publicDataOnly: options.publicDataOnly === true || !matches(rule?.privateEligible ?? rules.default.privateEligible),
  };
};
