import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  buildWooV3CancelAllAfterRequest,
  decodeWooV3CancelAllAfterResponse,
  WOO_V3_CANCEL_ALL_AFTER_LIMITS,
  WOO_V3_CANCEL_ALL_AFTER_PARITY,
} from '../reference/woo/woo.v3.cancel-all-after';
import {
  WOO_V3_ENDPOINTS,
  WOO_V3_MIGRATION_GATES,
  WOO_V3_MIGRATION_ROUTES,
} from '../reference/woo/woo.v3.contracts';
import { prepareWooV3Request } from '../reference/woo/woo.v3.request';
import { wooV3CancelAllAfterFixtures } from './woo.v3-cancel-all-after.fixtures';

const auth = {
  apiKey: 'AbmyVJGUpN064ks5ELjLfA==',
  apiSecret: 'QHKRXHPAW1MC9YGZMAT8YDJG2HPR',
  timestamp: 1_578_565_539_808,
} as const;

describe('WooX V3 inert Cancel-All-After contract', () => {
  it('records the exact official endpoint, migration route, dedicated false gate, and VIP semantics', () => {
    expect(WOO_V3_ENDPOINTS.TRADE_CANCEL_ALL_AFTER).toBe('/v3/trade/cancelAllAfter');
    expect(WOO_V3_MIGRATION_ROUTES).toContainEqual({
      id: 'cancel-all-after',
      legacyMethod: 'POST',
      legacyPath: '/v1/order/cancel_all_after',
      v3Method: 'POST',
      v3Path: '/v3/trade/cancelAllAfter',
      gate: 'cancelAllAfterDemoLifecycle',
      consumer: 'dead-man-switch (not currently owned by safe-cex runtime)',
      runtimeLifecycle: 'ownerless',
    });
    expect(WOO_V3_MIGRATION_GATES.cancelAllAfterDemoLifecycle).toBe(false);
    expect(WOO_V3_CANCEL_ALL_AFTER_PARITY).toEqual({
      vipOnly: true,
      cancelsOrdinaryOrders: true,
      cancelsAlgoOrders: true,
      zeroCancelsTimer: true,
      repeatedCallReplacesTimer: true,
      requestAndResponseFixtures: true,
      runtimeImported: false,
      transportOwner: false,
      credentialedDemoLifecycle: false,
    });
    expect(WOO_V3_CANCEL_ALL_AFTER_LIMITS).toEqual({
      minimumTriggerAfterMs: 0,
      maximumTriggerAfterMs: 900_000,
      requestsPerSecond: 10,
    });
  });

  it('builds and signs exact descriptors for disabling and the maximum timeout', () => {
    expect(buildWooV3CancelAllAfterRequest(0)).toEqual({
      method: 'POST',
      path: '/v3/trade/cancelAllAfter',
      body: { triggerAfter: 0 },
    });

    const descriptor = buildWooV3CancelAllAfterRequest(900_000);
    expect(descriptor.body).toEqual({ triggerAfter: 900_000 });
    const signed = prepareWooV3Request(descriptor, auth);
    expect(signed.body).toBe('{"triggerAfter":900000}');
    expect(signed.signaturePayload).toBe(
      `${auth.timestamp}POST/v3/trade/cancelAllAfter{"triggerAfter":900000}`,
    );
  });

  it('accepts only integer millisecond offsets in the inclusive official range', () => {
    for (const invalid of [-1, 900_001, 0.1, Number.NaN, Number.POSITIVE_INFINITY, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => buildWooV3CancelAllAfterRequest(invalid)).toThrow('safe integer from 0 through 900000');
    }
    expect(() => buildWooV3CancelAllAfterRequest('900000' as unknown as number)).toThrow(
      'safe integer from 0 through 900000',
    );
  });

  it('strictly decodes exact success fixtures without capping absolute expectedTriggerTime', () => {
    expect(decodeWooV3CancelAllAfterResponse(wooV3CancelAllAfterFixtures.disableTimerResponse)).toEqual({
      accepted: true,
      timestamp: 1_725_000_000_000,
      expectedTriggerTime: 0,
    });
    expect(decodeWooV3CancelAllAfterResponse(wooV3CancelAllAfterFixtures.scheduledResponse)).toEqual({
      accepted: true,
      timestamp: 1_725_000_000_000,
      expectedTriggerTime: 1_725_000_900_000,
    });
  });

  it('rejects failure envelopes, missing/additive keys, and unsafe server timestamps', () => {
    const valid = wooV3CancelAllAfterFixtures.scheduledResponse;
    expect(() => decodeWooV3CancelAllAfterResponse({ ...valid, success: false })).toThrow('success must be true');
    expect(() => decodeWooV3CancelAllAfterResponse({ timestamp: valid.timestamp, data: valid.data })).toThrow(
      'exactly data, success, timestamp',
    );
    expect(() => decodeWooV3CancelAllAfterResponse({ ...valid, code: 0 })).toThrow(
      'exactly data, success, timestamp',
    );
    expect(() =>
      decodeWooV3CancelAllAfterResponse({ ...valid, data: { ...valid.data, triggerAfter: 900_000 } }),
    ).toThrow('exactly expectedTriggerTime');
    expect(() => decodeWooV3CancelAllAfterResponse({ ...valid, timestamp: 1.5 })).toThrow(
      'nonnegative safe integer',
    );
    expect(() => decodeWooV3CancelAllAfterResponse({ ...valid, timestamp: -1 })).toThrow(
      'nonnegative safe integer',
    );
    expect(() =>
      decodeWooV3CancelAllAfterResponse({
        ...valid,
        data: { expectedTriggerTime: Number.MAX_SAFE_INTEGER + 1 },
      }),
    ).toThrow('nonnegative safe integer');
  });

  it('remains disconnected from every active Woo runtime and owns no transport or selector', () => {
    const runtimeFiles = ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-public.ts', 'woo.ws-private.ts'];
    for (const file of runtimeFiles) {
      const source = readFileSync(join(__dirname, '../reference/woo', file), 'utf8');
      expect(source).not.toContain('woo.v3.cancel-all-after');
      expect(source).not.toContain('TRADE_CANCEL_ALL_AFTER');
      expect(source).not.toContain('cancelAllAfterDemoLifecycle');
    }
    expect(WOO_V3_CANCEL_ALL_AFTER_PARITY.runtimeImported).toBe(false);
    expect(WOO_V3_CANCEL_ALL_AFTER_PARITY.transportOwner).toBe(false);
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([
      false,
      false,
      false,
      false,
      false,
      false,
      false,
      false,
    ]);
  });
});
