import type { CoinbaseStarbaseCutoverEvidence } from '../reference/coinbase/coinbase.starbase-cutover';

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { CoinbaseStarbaseProtocolError } from '../reference/coinbase/coinbase.starbase';
import {
  COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE,
  COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED,
  COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED,
  COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED,
  evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly,
  selectCoinbaseStarbaseCutoverPlan,
} from '../reference/coinbase/coinbase.starbase-cutover';

const HTTP_SHADOW_READY_EVIDENCE: CoinbaseStarbaseCutoverEvidence = {
  ...COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE,
  credentialedHttpAuthentication: 'verified',
};

describe('Coinbase Starbase inert cutover gate', () => {
  it('distinguishes implemented contracts from unresolved live verification', () => {
    expect(COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE).toMatchObject({
      httpAuthenticationContract: 'implemented',
      privateHttpResponseContracts: 'implemented',
      publicHttpResponseContracts: 'implemented',
      websocketSessionContract: 'implemented',
    });
    expect([
      COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE.credentialedHttpAuthentication,
      COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE.credentialedHttpReadParity,
      COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE.credentialedWebsocketSessionLifecycle,
      COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE.credentialedWriteCanary,
      COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE.cutoverRunbook,
      COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE.privateNotificationEnvelope,
      COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE.unitParity,
    ]).toEqual(Array(7).fill('unverified'));
    expect(COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED).toBe(false);
    expect(COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED).toBe(false);
    expect(COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED).toBe(false);
  });

  it('keeps the enforcement selector product-unbound and closed over repository evidence', () => {
    expect(selectCoinbaseStarbaseCutoverPlan.length).toBe(0);
    const plan = selectCoinbaseStarbaseCutoverPlan();

    expect(plan.productBinding).toEqual({
      required: 'trusted-runtime-intx-portfolio-binding',
      status: 'unbound',
    });
    expect(plan.authoritative).toEqual({
      privateReads: 'legacy-coinbase',
      publicReads: 'legacy-coinbase',
      writes: 'legacy-coinbase',
    });
    expect(plan.httpShadow).toMatchObject({
      active: false,
      eligible: false,
      runtimeActivationEnabled: false,
    });
    expect(plan.httpShadow.blockers).toContain('runtime-product-binding-unavailable');
    expect(plan.cutover).toMatchObject({ eligible: false, runtimeActivationEnabled: false });
  });

  it('describes only an HTTP JSON-RPC result shadow in the test-only evaluator', () => {
    const readiness = evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly({
      evidence: HTTP_SHADOW_READY_EVIDENCE,
      productClassification: 'intx',
    });

    expect(readiness).toEqual({
      blockers: [],
      eligible: true,
      productClassification: 'intx',
      plan: {
        authentication: 'starbase-http',
        privateReads: 'starbase-http-json-rpc-results',
        publicReads: 'starbase-http-json-rpc-results',
        transport: 'http',
        writes: 'disabled',
      },
    });
    expect(JSON.stringify(readiness)).not.toMatch(/websocket|session|subscription|notification/i);
  });

  it('keeps spot and CFM outside the test-only INTX HTTP shadow', () => {
    for (const productClassification of ['spot', 'cfm'] as const) {
      const readiness = evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly({
        evidence: HTTP_SHADOW_READY_EVIDENCE,
        productClassification,
      });
      expect(readiness).toEqual({
        blockers: ['product-outside-intx-migration'],
        eligible: false,
        productClassification,
      });
    }
  });

  it('requires every HTTP contract and credentialed HTTP authentication prerequisite atomically', () => {
    const prerequisites = {
      credentialedHttpAuthentication: ['unverified', 'credentialed-http-authentication-unverified'],
      httpAuthenticationContract: ['not-implemented', 'http-authentication-contract-not-implemented'],
      privateHttpResponseContracts: ['not-implemented', 'private-http-response-contracts-not-implemented'],
      publicHttpResponseContracts: ['not-implemented', 'public-http-response-contracts-not-implemented'],
    } as const;

    for (const [key, [status, blocker]] of Object.entries(prerequisites)) {
      const readiness = evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly({
        evidence: { ...HTTP_SHADOW_READY_EVIDENCE, [key]: status },
        productClassification: 'intx',
      });
      expect(readiness.eligible).toBe(false);
      expect(readiness.plan).toBeUndefined();
      expect(readiness.blockers).toEqual([blocker]);
    }
  });

  it('keeps WebSocket session, subscriptions, and private notifications separately closed', () => {
    const plan = selectCoinbaseStarbaseCutoverPlan();
    expect(plan.websocket).toEqual({
      active: false,
      blockers: [
        'credentialed-websocket-session-lifecycle-unverified',
        'private-notification-envelope-unverified',
        'websocket-activation-disabled',
      ],
      privateNotifications: 'disabled',
      publicSubscriptions: 'disabled',
      runtimeActivationEnabled: false,
      session: 'disabled',
    });
  });

  it('rejects missing, unknown, and incorrectly typed evidence keys', () => {
    expect(() =>
      evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly({
        evidence: {
          ...HTTP_SHADOW_READY_EVIDENCE,
          inventedGate: 'verified',
        } as CoinbaseStarbaseCutoverEvidence,
        productClassification: 'intx',
      }),
    ).toThrow('unknown key inventedGate');
    const evidenceWithSymbol = { ...HTTP_SHADOW_READY_EVIDENCE } as CoinbaseStarbaseCutoverEvidence & {
      [key: symbol]: string;
    };
    evidenceWithSymbol[Symbol('invented')] = 'verified';
    expect(() =>
      evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly({
        evidence: evidenceWithSymbol,
        productClassification: 'intx',
      }),
    ).toThrow('unknown key Symbol(invented)');
    const { credentialedHttpAuthentication: _missing, ...missingEvidence } = HTTP_SHADOW_READY_EVIDENCE;
    expect(() =>
      evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly({
        evidence: missingEvidence as CoinbaseStarbaseCutoverEvidence,
        productClassification: 'intx',
      }),
    ).toThrow('missing key credentialedHttpAuthentication');
    expect(() =>
      evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly({
        evidence: {
          ...HTTP_SHADOW_READY_EVIDENCE,
          httpAuthenticationContract: 'verified',
        } as unknown as CoinbaseStarbaseCutoverEvidence,
        productClassification: 'intx',
      }),
    ).toThrow(CoinbaseStarbaseProtocolError);
  });

  it('rejects invented product classifications in the test-only evaluator', () => {
    expect(() =>
      evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly({
        evidence: HTTP_SHADOW_READY_EVIDENCE,
        productClassification: 'options' as 'intx',
      }),
    ).toThrow('Invalid test-only Starbase product classification');
  });

  it('remains absent from active Coinbase runtime, metadata, registry, and routing', () => {
    for (const file of [
      '../../__tests__/reference/coinbase/coinbase.api.ts',
      '../../__tests__/reference/coinbase/coinbase.exchange.ts',
      '../../__tests__/reference/coinbase/coinbase.ws-private.ts',
      '../../__tests__/reference/coinbase/coinbase.ws-public.ts',
      '../../createExchangeFromAccount.ts',
      '../../exchangeMetadata.ts',
      '../registry.native.ts',
      '../registry.ts',
      '../../transport/tealstreet/routing/routingTable.ts',
    ]) {
      const source = readFileSync(resolve(__dirname, '../../exchanges/coinbase', file), 'utf8');
      expect(source).not.toContain('coinbase.starbase-cutover');
    }
  });
});
