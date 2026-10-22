import type { EngineBridgeConstruction, EngineConstruction } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';

import rules from './construction-rules.json';
import { hasAsterDEXV3AuthContext, resolveAsterDEXApiVersion } from '../asterdex/asterdex.types';

type Rule = {
  canSign?: string[][]; privateEligible?: string[][]; explicitPrivateError?: string;
  bridge?: Partial<Omit<EngineBridgeConstruction, 'demo'>> & { demo?: 'always' | 'hyroMode' };
};

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
): Required<Pick<EngineConstruction, 'canSign' | 'publicDataOnly'>> => {
  const rule = (rules.venues as Record<string, Rule>)[name];
  const inputs: Record<string, boolean> = {
    key: Boolean(options.key),
    secret: Boolean(options.secret),
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
