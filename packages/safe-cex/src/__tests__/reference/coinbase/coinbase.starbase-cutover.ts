/**
 * Inert cutover assessment for Coinbase's Starbase migration.
 *
 * The enforcement-facing selector accepts no caller evidence or product. It
 * reads only repository-owned evidence and remains product-unbound until a
 * future runtime integration derives INTX identity from a trusted portfolio.
 * Nothing in this module creates a transport, mutates a store, or routes work.
 */

import { CoinbaseStarbaseProtocolError } from './coinbase.starbase';

const IMPLEMENTATION_STATUSES = ['not-implemented', 'implemented'] as const;
const VERIFICATION_STATUSES = ['unverified', 'verified'] as const;
const TEST_PRODUCT_CLASSIFICATIONS = ['spot', 'cfm', 'intx'] as const;

export type CoinbaseStarbaseImplementationStatus = (typeof IMPLEMENTATION_STATUSES)[number];
export type CoinbaseStarbaseVerificationStatus = (typeof VERIFICATION_STATUSES)[number];

/**
 * Implementation statuses are deterministic repository facts. Verification
 * statuses require credentialed/live evidence and must not be inferred from
 * the presence of an implementation.
 */
export type CoinbaseStarbaseCutoverEvidence = {
  credentialedHttpAuthentication: CoinbaseStarbaseVerificationStatus;
  credentialedHttpReadParity: CoinbaseStarbaseVerificationStatus;
  credentialedWebsocketSessionLifecycle: CoinbaseStarbaseVerificationStatus;
  credentialedWriteCanary: CoinbaseStarbaseVerificationStatus;
  cutoverRunbook: CoinbaseStarbaseVerificationStatus;
  httpAuthenticationContract: CoinbaseStarbaseImplementationStatus;
  privateHttpResponseContracts: CoinbaseStarbaseImplementationStatus;
  privateNotificationEnvelope: CoinbaseStarbaseVerificationStatus;
  publicHttpResponseContracts: CoinbaseStarbaseImplementationStatus;
  unitParity: CoinbaseStarbaseVerificationStatus;
  websocketSessionContract: CoinbaseStarbaseImplementationStatus;
};

export const COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE: Readonly<CoinbaseStarbaseCutoverEvidence> = Object.freeze({
  credentialedHttpAuthentication: 'unverified',
  credentialedHttpReadParity: 'unverified',
  credentialedWebsocketSessionLifecycle: 'unverified',
  credentialedWriteCanary: 'unverified',
  cutoverRunbook: 'unverified',
  httpAuthenticationContract: 'implemented',
  privateHttpResponseContracts: 'implemented',
  privateNotificationEnvelope: 'unverified',
  publicHttpResponseContracts: 'implemented',
  unitParity: 'unverified',
  websocketSessionContract: 'implemented',
});

// These are intentionally not runtime-configurable. Future activation requires
// reviewed code plus trusted runtime portfolio binding after every evidence gate.
export const COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED = false as const;
export const COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED = false as const;
export const COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED = false as const;

export type CoinbaseStarbaseHttpShadowBlocker =
  | 'credentialed-http-authentication-unverified'
  | 'http-authentication-contract-not-implemented'
  | 'private-http-response-contracts-not-implemented'
  | 'product-outside-intx-migration'
  | 'public-http-response-contracts-not-implemented';

export type CoinbaseStarbaseWebsocketBlocker =
  | 'credentialed-websocket-session-lifecycle-unverified'
  | 'private-notification-envelope-unverified'
  | 'websocket-activation-disabled'
  | 'websocket-session-contract-not-implemented';

export type CoinbaseStarbaseCutoverBlocker =
  | CoinbaseStarbaseHttpShadowBlocker
  | CoinbaseStarbaseWebsocketBlocker
  | 'credentialed-http-read-parity-unverified'
  | 'credentialed-write-canary-unverified'
  | 'cutover-runbook-unverified'
  | 'runtime-activation-disabled'
  | 'runtime-product-binding-unavailable'
  | 'unit-parity-unverified';

type CoinbaseStarbaseTestProductClassification = (typeof TEST_PRODUCT_CLASSIFICATIONS)[number];

export type CoinbaseStarbaseHttpShadowReadiness = {
  eligible: boolean;
  productClassification: CoinbaseStarbaseTestProductClassification;
  blockers: CoinbaseStarbaseHttpShadowBlocker[];
  plan?: {
    authentication: 'starbase-http';
    privateReads: 'starbase-http-json-rpc-results';
    publicReads: 'starbase-http-json-rpc-results';
    transport: 'http';
    writes: 'disabled';
  };
};

export type CoinbaseStarbaseCutoverPlan = {
  productBinding: {
    required: 'trusted-runtime-intx-portfolio-binding';
    status: 'unbound';
  };
  authoritative: {
    privateReads: 'legacy-coinbase';
    publicReads: 'legacy-coinbase';
    writes: 'legacy-coinbase';
  };
  httpShadow: {
    active: false;
    eligible: false;
    runtimeActivationEnabled: false;
    blockers: Array<CoinbaseStarbaseHttpShadowBlocker | 'runtime-product-binding-unavailable'>;
  };
  websocket: {
    active: false;
    privateNotifications: 'disabled';
    publicSubscriptions: 'disabled';
    runtimeActivationEnabled: false;
    session: 'disabled';
    blockers: CoinbaseStarbaseWebsocketBlocker[];
  };
  cutover: {
    eligible: false;
    runtimeActivationEnabled: false;
    blockers: CoinbaseStarbaseCutoverBlocker[];
  };
};

const EVIDENCE_KEYS = [
  'credentialedHttpAuthentication',
  'credentialedHttpReadParity',
  'credentialedWebsocketSessionLifecycle',
  'credentialedWriteCanary',
  'cutoverRunbook',
  'httpAuthenticationContract',
  'privateHttpResponseContracts',
  'privateNotificationEnvelope',
  'publicHttpResponseContracts',
  'unitParity',
  'websocketSessionContract',
] as const satisfies readonly (keyof CoinbaseStarbaseCutoverEvidence)[];

const validateEvidence = (value: CoinbaseStarbaseCutoverEvidence): void => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new CoinbaseStarbaseProtocolError('Invalid Starbase cutover evidence');
  }

  const actualKeys = Reflect.ownKeys(value);
  const unknownKey = actualKeys.find(
    (key) => typeof key !== 'string' || !EVIDENCE_KEYS.includes(key as keyof CoinbaseStarbaseCutoverEvidence),
  );
  if (unknownKey) {
    throw new CoinbaseStarbaseProtocolError(`Invalid Starbase cutover evidence: unknown key ${String(unknownKey)}`);
  }
  const missingKey = EVIDENCE_KEYS.find((key) => !Object.prototype.hasOwnProperty.call(value, key));
  if (missingKey) {
    throw new CoinbaseStarbaseProtocolError(`Invalid Starbase cutover evidence: missing key ${missingKey}`);
  }

  for (const key of EVIDENCE_KEYS) {
    const status = value[key];
    const valid =
      key.endsWith('Contract') || key.endsWith('Contracts')
        ? IMPLEMENTATION_STATUSES.includes(status as CoinbaseStarbaseImplementationStatus)
        : VERIFICATION_STATUSES.includes(status as CoinbaseStarbaseVerificationStatus);
    if (!valid) {
      throw new CoinbaseStarbaseProtocolError(`Invalid Starbase cutover evidence: ${key}`);
    }
  }
};

const collectHttpShadowBlockers = (
  productClassification: CoinbaseStarbaseTestProductClassification,
  evidence: CoinbaseStarbaseCutoverEvidence,
): CoinbaseStarbaseHttpShadowBlocker[] => {
  if (productClassification !== 'intx') return ['product-outside-intx-migration'];

  const blockers: CoinbaseStarbaseHttpShadowBlocker[] = [];
  if (evidence.publicHttpResponseContracts !== 'implemented') {
    blockers.push('public-http-response-contracts-not-implemented');
  }
  if (evidence.privateHttpResponseContracts !== 'implemented') {
    blockers.push('private-http-response-contracts-not-implemented');
  }
  if (evidence.httpAuthenticationContract !== 'implemented') {
    blockers.push('http-authentication-contract-not-implemented');
  }
  if (evidence.credentialedHttpAuthentication !== 'verified') {
    blockers.push('credentialed-http-authentication-unverified');
  }
  return blockers;
};

const collectWebsocketBlockers = (evidence: CoinbaseStarbaseCutoverEvidence): CoinbaseStarbaseWebsocketBlocker[] => {
  const blockers: CoinbaseStarbaseWebsocketBlocker[] = [];
  if (evidence.websocketSessionContract !== 'implemented') {
    blockers.push('websocket-session-contract-not-implemented');
  }
  if (evidence.credentialedWebsocketSessionLifecycle !== 'verified') {
    blockers.push('credentialed-websocket-session-lifecycle-unverified');
  }
  if (evidence.privateNotificationEnvelope !== 'verified') {
    blockers.push('private-notification-envelope-unverified');
  }
  blockers.push('websocket-activation-disabled');
  return blockers;
};

/**
 * TEST-ONLY pure evaluator for synthetic evidence. This is not an enforcement
 * selector and is not imported by any runtime. It can describe an HTTP-only
 * comparison plan; it never describes a WebSocket or authoritative route.
 */
export const evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly = (options: {
  evidence: CoinbaseStarbaseCutoverEvidence;
  productClassification: CoinbaseStarbaseTestProductClassification;
}): CoinbaseStarbaseHttpShadowReadiness => {
  validateEvidence(options.evidence);
  if (!TEST_PRODUCT_CLASSIFICATIONS.includes(options.productClassification)) {
    throw new CoinbaseStarbaseProtocolError('Invalid test-only Starbase product classification');
  }

  const blockers = collectHttpShadowBlockers(options.productClassification, options.evidence);
  return {
    eligible: blockers.length === 0,
    productClassification: options.productClassification,
    blockers,
    ...(blockers.length === 0
      ? {
          plan: {
            authentication: 'starbase-http' as const,
            privateReads: 'starbase-http-json-rpc-results' as const,
            publicReads: 'starbase-http-json-rpc-results' as const,
            transport: 'http' as const,
            writes: 'disabled' as const,
          },
        }
      : {}),
  };
};

/**
 * Enforcement-facing selector. It deliberately accepts no arguments: callers
 * cannot inject evidence or claim a product identity. Until a future trusted
 * portfolio binding exists, it can only return the closed legacy plan.
 */
export const selectCoinbaseStarbaseCutoverPlan = (): CoinbaseStarbaseCutoverPlan => {
  const evidence = COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE;
  validateEvidence(evidence);
  const httpBlockers = collectHttpShadowBlockers('intx', evidence);
  const websocketBlockers = collectWebsocketBlockers(evidence);
  const cutoverBlockers: CoinbaseStarbaseCutoverBlocker[] = ['runtime-product-binding-unavailable', ...httpBlockers];
  if (evidence.unitParity !== 'verified') cutoverBlockers.push('unit-parity-unverified');
  if (evidence.credentialedHttpReadParity !== 'verified') {
    cutoverBlockers.push('credentialed-http-read-parity-unverified');
  }
  cutoverBlockers.push(...websocketBlockers);
  if (evidence.credentialedWriteCanary !== 'verified') {
    cutoverBlockers.push('credentialed-write-canary-unverified');
  }
  if (evidence.cutoverRunbook !== 'verified') cutoverBlockers.push('cutover-runbook-unverified');
  cutoverBlockers.push('runtime-activation-disabled');

  return {
    productBinding: {
      required: 'trusted-runtime-intx-portfolio-binding',
      status: 'unbound',
    },
    authoritative: {
      privateReads: 'legacy-coinbase',
      publicReads: 'legacy-coinbase',
      writes: 'legacy-coinbase',
    },
    httpShadow: {
      active: false,
      eligible: false,
      runtimeActivationEnabled: COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED,
      blockers: ['runtime-product-binding-unavailable', ...httpBlockers],
    },
    websocket: {
      active: false,
      privateNotifications: 'disabled',
      publicSubscriptions: 'disabled',
      runtimeActivationEnabled: COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED,
      session: 'disabled',
      blockers: websocketBlockers,
    },
    cutover: {
      eligible: false,
      runtimeActivationEnabled: COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED,
      blockers: cutoverBlockers,
    },
  };
};
