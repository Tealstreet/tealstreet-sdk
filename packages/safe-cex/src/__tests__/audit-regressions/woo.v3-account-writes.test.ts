import type {
  WooV3AccountModeProvenance,
  WooV3CompletePositionModeEvidence,
} from '../reference/woo/woo.v3.account-writes';
import type { WooV3AlgoOrder, WooV3FuturesPosition, WooV3RegularOrder } from '../reference/woo/woo.v3.private';

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  buildWooV3GetLeverageRequest,
  buildWooV3SetLeverageRequest,
  buildWooV3SetPositionModeRequest,
  decodeWooV3GetLeverageResponse,
  decodeWooV3SetLeverageResponse,
  decodeWooV3SetPositionModeResponse,
  WOO_V3_ACCOUNT_WRITE_PARITY,
  WooV3AccountConfigResponseError,
} from '../reference/woo/woo.v3.account-writes';
import { WOO_V3_MIGRATION_GATES } from '../reference/woo/woo.v3.contracts';
import { prepareWooV3Request } from '../reference/woo/woo.v3.request';

const auth = {
  apiKey: 'AbmyVJGUpN064ks5ELjLfA==',
  apiSecret: 'QHKRXHPAW1MC9YGZMAT8YDJG2HPR',
  timestamp: 1_578_565_539_808,
} as const;

const account = (overrides: Partial<WooV3AccountModeProvenance> = {}): WooV3AccountModeProvenance => ({
  source: 'decoded-v3-account-info',
  reconciliationId: 'reconcile-42',
  accountMode: 'MARGIN',
  positionMode: 'ONE_WAY',
  ...overrides,
});

const evidence = (overrides: Partial<WooV3CompletePositionModeEvidence> = {}): WooV3CompletePositionModeEvidence => ({
  account: account(),
  positions: {
    source: 'decoded-v3-futures-positions-complete-page',
    reconciliationId: 'reconcile-42',
    complete: true,
    rows: [],
  },
  regularOrders: {
    source: 'decoded-v3-pending-regular-orders-complete-pages',
    reconciliationId: 'reconcile-42',
    complete: true,
    rows: [],
  },
  algoOrders: {
    source: 'decoded-v3-pending-algo-orders-complete-pages',
    reconciliationId: 'reconcile-42',
    complete: true,
    rows: [],
  },
  ...overrides,
});

const position = (overrides: Partial<WooV3FuturesPosition> = {}): WooV3FuturesPosition => ({
  symbol: 'PERP_BTC_USDT',
  holding: '0',
  positionSide: 'BOTH',
  pendingLongQty: '0.000',
  pendingShortQty: '-0',
  settlePrice: '60000',
  averageOpenPrice: '0',
  pnl24H: '0',
  fee24H: '0',
  markPrice: '60000',
  estLiqPrice: '0',
  adlQuantile: 0,
  timestamp: 1_725_000_000_000,
  ...overrides,
});

const leverageResponse = (
  positionMode: 'ONE_WAY' | 'HEDGE_MODE',
  details: readonly Record<string, unknown>[],
  overrides: Record<string, unknown> = {},
) => ({
  success: true,
  data: {
    symbol: 'PERP_BTC_USDT',
    marginMode: 'CROSS',
    positionMode,
    details,
    ...overrides,
  },
  timestamp: 1_725_000_000_000,
});

describe('WooX V3 inert account writes', () => {
  it('builds and signs the exact leverage readback query', () => {
    const built = buildWooV3GetLeverageRequest({
      symbol: 'PERP_BTC_USDT',
      marginMode: 'CROSS',
      positionMode: 'ONE_WAY',
    });
    expect(built).toEqual({
      request: {
        method: 'GET',
        path: '/v3/futures/leverage',
        query: { symbol: 'PERP_BTC_USDT', marginMode: 'CROSS', positionMode: 'ONE_WAY' },
      },
      target: { symbol: 'PERP_BTC_USDT', marginMode: 'CROSS', positionMode: 'ONE_WAY' },
    });
    expect(prepareWooV3Request(built.request, auth)).toMatchObject({
      path: '/v3/futures/leverage?marginMode=CROSS&positionMode=ONE_WAY&symbol=PERP_BTC_USDT',
      signaturePayload: `${auth.timestamp}GET/v3/futures/leverage?marginMode=CROSS&positionMode=ONE_WAY&symbol=PERP_BTC_USDT`,
    });
  });

  it('strictly decodes the unique BOTH detail for one-way leverage readback', () => {
    const built = buildWooV3GetLeverageRequest({
      symbol: 'PERP_BTC_USDT',
      marginMode: 'CROSS',
      positionMode: 'ONE_WAY',
    });
    expect(
      decodeWooV3GetLeverageResponse(leverageResponse('ONE_WAY', [{ positionSide: 'BOTH', leverage: 10 }]), built),
    ).toEqual({
      symbol: 'PERP_BTC_USDT',
      marginMode: 'CROSS',
      positionMode: 'ONE_WAY',
      details: [{ positionSide: 'BOTH', leverage: 10 }],
      timestamp: 1_725_000_000_000,
    });
  });

  it('strictly decodes exactly one unique LONG and SHORT hedge leverage detail', () => {
    const built = buildWooV3GetLeverageRequest({
      symbol: 'PERP_BTC_USDT',
      marginMode: 'CROSS',
      positionMode: 'HEDGE_MODE',
    });
    expect(
      decodeWooV3GetLeverageResponse(
        leverageResponse('HEDGE_MODE', [
          { positionSide: 'LONG', leverage: 10 },
          { positionSide: 'SHORT', leverage: 15 },
        ]),
        built,
      ),
    ).toEqual({
      symbol: 'PERP_BTC_USDT',
      marginMode: 'CROSS',
      positionMode: 'HEDGE_MODE',
      details: [
        { positionSide: 'LONG', leverage: 10 },
        { positionSide: 'SHORT', leverage: 15 },
      ],
      timestamp: 1_725_000_000_000,
    });
  });

  it('rejects mismatched scope, ambiguous legs, invalid leverage, and extra readback fields', () => {
    const oneWay = buildWooV3GetLeverageRequest({
      symbol: 'PERP_BTC_USDT',
      marginMode: 'CROSS',
      positionMode: 'ONE_WAY',
    });
    expect(() =>
      decodeWooV3GetLeverageResponse(
        leverageResponse('ONE_WAY', [{ positionSide: 'BOTH', leverage: 10 }], { symbol: 'PERP_ETH_USDT' }),
        oneWay,
      ),
    ).toThrow('scope must match');
    expect(() =>
      decodeWooV3GetLeverageResponse(
        leverageResponse('ONE_WAY', [
          { positionSide: 'BOTH', leverage: 10 },
          { positionSide: 'BOTH', leverage: 10 },
        ]),
        oneWay,
      ),
    ).toThrow('exactly one unique BOTH');
    expect(() =>
      decodeWooV3GetLeverageResponse(leverageResponse('ONE_WAY', [{ positionSide: 'BOTH', leverage: 0 }]), oneWay),
    ).toThrow('safe integer of at least 1');
    expect(() =>
      decodeWooV3GetLeverageResponse(
        { ...leverageResponse('ONE_WAY', [{ positionSide: 'BOTH', leverage: 10 }]), defaultMarginMode: 'CROSS' },
        oneWay,
      ),
    ).toThrow('must contain exactly');
  });

  it('builds and signs the exact cross one-way leverage request without the ignored positionMode field', () => {
    const built = buildWooV3SetLeverageRequest({
      symbol: 'PERP_BTC_USDT',
      marginMode: 'CROSS',
      leverage: 20,
      account: account(),
    });
    expect(built).toEqual({
      ready: true,
      request: {
        method: 'PUT',
        path: '/v3/futures/leverage',
        body: { symbol: 'PERP_BTC_USDT', marginMode: 'CROSS', leverage: 20 },
      },
      target: { symbol: 'PERP_BTC_USDT', marginMode: 'CROSS', positionMode: 'ONE_WAY', leverage: 20 },
      scopeEvidence: 'official-cross-setter-ignores-positionMode-one-way-account-provenance',
    });
    if (!built.ready) throw new Error('expected ready leverage fixture');
    const prepared = prepareWooV3Request(built.request, auth);
    expect(prepared.body).toBe('{"symbol":"PERP_BTC_USDT","marginMode":"CROSS","leverage":20}');
    expect(prepared.signaturePayload).toBe(`${auth.timestamp}PUT/v3/futures/leverage${prepared.body}`);
  });

  it('requires a PERP symbol and a positive safe-integer leverage', () => {
    expect(() =>
      buildWooV3SetLeverageRequest({ symbol: 'SPOT_BTC_USDT', marginMode: 'CROSS', leverage: 20, account: account() }),
    ).toThrow('PERP_BASE_QUOTE');
    for (const leverage of [0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() =>
        buildWooV3SetLeverageRequest({ symbol: 'PERP_BTC_USDT', marginMode: 'CROSS', leverage, account: account() }),
      ).toThrow('safe integer of at least 1');
    }
  });

  it('blocks spot-account, isolated-margin, and cross-hedge leverage ambiguity', () => {
    expect(
      buildWooV3SetLeverageRequest({
        symbol: 'PERP_BTC_USDT',
        marginMode: 'CROSS',
        leverage: 20,
        account: account({ accountMode: 'SPOT' }),
      }),
    ).toMatchObject({ ready: false, blocker: 'futures-margin-account-mode-not-proven' });
    expect(
      buildWooV3SetLeverageRequest({
        symbol: 'PERP_BTC_USDT',
        marginMode: 'ISOLATED',
        leverage: 20,
        account: account(),
      }),
    ).toMatchObject({ ready: false, blocker: 'isolated-margin-outside-safe-cex-contract' });
    expect(
      buildWooV3SetLeverageRequest({
        symbol: 'PERP_BTC_USDT',
        marginMode: 'CROSS',
        leverage: 20,
        account: account({ positionMode: 'HEDGE_MODE' }),
      }),
    ).toMatchObject({ ready: false, blocker: 'cross-hedge-resulting-leg-scope-requires-demo-evidence' });
  });

  it('builds and signs position-mode changes only from complete reconciled empty snapshots', () => {
    const built = buildWooV3SetPositionModeRequest({ targetPositionMode: 'HEDGE_MODE', evidence: evidence() });
    expect(built).toEqual({
      ready: true,
      request: { method: 'PUT', path: '/v3/futures/positionMode', body: { positionMode: 'HEDGE_MODE' } },
      currentPositionMode: 'ONE_WAY',
      targetPositionMode: 'HEDGE_MODE',
      preconditionEvidence: 'complete-reconciled-v3-positions-zero-and-pending-orders-empty',
    });
    if (!built.ready) throw new Error('expected ready position-mode fixture');
    expect(prepareWooV3Request(built.request, auth).signaturePayload).toBe(
      `${auth.timestamp}PUT/v3/futures/positionMode{"positionMode":"HEDGE_MODE"}`,
    );
  });

  it('rejects incomplete, fabricated, or mixed-reconciliation position-mode evidence', () => {
    expect(
      buildWooV3SetPositionModeRequest({
        targetPositionMode: 'HEDGE_MODE',
        evidence: undefined as unknown as WooV3CompletePositionModeEvidence,
      }),
    ).toMatchObject({ ready: false, blocker: 'account-state-evidence-incomplete-or-mixed' });
    expect(
      buildWooV3SetPositionModeRequest({
        targetPositionMode: 'HEDGE_MODE',
        evidence: evidence({
          positions: {
            source: 'decoded-v3-futures-positions-complete-page',
            reconciliationId: 'different-snapshot',
            complete: true,
            rows: [],
          },
        }),
      }),
    ).toMatchObject({ ready: false, blocker: 'account-state-evidence-incomplete-or-mixed' });
    expect(
      buildWooV3SetPositionModeRequest({
        targetPositionMode: 'HEDGE_MODE',
        evidence: evidence({
          regularOrders: {
            source: 'caller-asserted-empty' as 'decoded-v3-pending-regular-orders-complete-pages',
            reconciliationId: 'reconcile-42',
            complete: true,
            rows: [],
          },
        }),
      }),
    ).toMatchObject({ ready: false, blocker: 'account-state-evidence-incomplete-or-mixed' });
  });

  it('blocks no-op changes and non-margin account provenance', () => {
    expect(buildWooV3SetPositionModeRequest({ targetPositionMode: 'ONE_WAY', evidence: evidence() })).toMatchObject({
      ready: false,
      blocker: 'position-mode-already-active',
    });
    expect(
      buildWooV3SetPositionModeRequest({
        targetPositionMode: 'HEDGE_MODE',
        evidence: evidence({ account: account({ accountMode: 'SPOT' }) }),
      }),
    ).toMatchObject({ ready: false, blocker: 'futures-margin-account-mode-not-proven' });
  });

  it('accepts decoded zero-position rows but blocks nonzero or malformed exposure', () => {
    expect(
      buildWooV3SetPositionModeRequest({
        targetPositionMode: 'HEDGE_MODE',
        evidence: evidence({ positions: { ...evidence().positions, rows: [position()] } }),
      }),
    ).toMatchObject({ ready: true });
    expect(
      buildWooV3SetPositionModeRequest({
        targetPositionMode: 'HEDGE_MODE',
        evidence: evidence({ positions: { ...evidence().positions, rows: [position({ pendingLongQty: '0.001' })] } }),
      }),
    ).toMatchObject({ ready: false, blocker: 'open-positions-must-be-closed' });
    expect(
      buildWooV3SetPositionModeRequest({
        targetPositionMode: 'HEDGE_MODE',
        evidence: evidence({ positions: { ...evidence().positions, rows: [{} as WooV3FuturesPosition] } }),
      }),
    ).toMatchObject({ ready: false, blocker: 'open-positions-must-be-closed' });
  });

  it('requires both regular and algo pending-order sets to be empty', () => {
    expect(
      buildWooV3SetPositionModeRequest({
        targetPositionMode: 'HEDGE_MODE',
        evidence: evidence({ regularOrders: { ...evidence().regularOrders, rows: [{} as WooV3RegularOrder] } }),
      }),
    ).toMatchObject({ ready: false, blocker: 'pending-regular-orders-must-be-cancelled' });
    expect(
      buildWooV3SetPositionModeRequest({
        targetPositionMode: 'HEDGE_MODE',
        evidence: evidence({ algoOrders: { ...evidence().algoOrders, rows: [{} as WooV3AlgoOrder] } }),
      }),
    ).toMatchObject({ ready: false, blocker: 'pending-algo-orders-must-be-cancelled' });
  });

  it('decodes only exact documented success envelopes and exposes request-only state evidence', () => {
    const leverage = buildWooV3SetLeverageRequest({
      symbol: 'PERP_BTC_USDT',
      marginMode: 'CROSS',
      leverage: 20,
      account: account(),
    });
    const positionMode = buildWooV3SetPositionModeRequest({ targetPositionMode: 'HEDGE_MODE', evidence: evidence() });
    if (!leverage.ready || !positionMode.ready) throw new Error('expected ready account-write fixtures');
    expect(decodeWooV3SetLeverageResponse({ success: true, timestamp: 1_725_000_000_000 }, leverage)).toEqual({
      accepted: true,
      timestamp: 1_725_000_000_000,
      target: { family: 'futures-leverage', ...leverage.target },
      stateEvidence: 'request-target-only-response-does-not-echo-resulting-state',
    });
    expect(decodeWooV3SetPositionModeResponse({ success: true, timestamp: 1_725_000_000_001 }, positionMode)).toEqual({
      accepted: true,
      timestamp: 1_725_000_000_001,
      target: { family: 'position-mode', positionMode: 'HEDGE_MODE' },
      stateEvidence: 'request-target-only-response-does-not-echo-resulting-state',
    });
    expect(() => decodeWooV3SetLeverageResponse({ success: true, timestamp: 1, data: {} }, leverage)).toThrow(
      'must contain exactly',
    );
    expect(() => decodeWooV3SetPositionModeResponse({ success: 'true', timestamp: 1 }, positionMode)).toThrow(
      'boolean',
    );
  });

  it('preserves undocumented vendor error evidence', () => {
    const built = buildWooV3SetLeverageRequest({
      symbol: 'PERP_BTC_USDT',
      marginMode: 'CROSS',
      leverage: 20,
      account: account(),
    });
    if (!built.ready) throw new Error('expected ready leverage fixture');
    try {
      decodeWooV3SetLeverageResponse(
        { success: false, code: 302_306, message: 'Leverage should be a positive integer', detail: 'vendor' },
        built,
      );
      throw new Error('expected vendor error');
    } catch (error) {
      expect(error).toBeInstanceOf(WooV3AccountConfigResponseError);
      expect(error).toMatchObject({
        code: 302_306,
        message: 'Leverage should be a positive integer',
        contractStatus: 'vendor-error-envelope-undocumented',
        gatewayResponse: expect.objectContaining({ detail: 'vendor' }),
      });
    }
  });

  it('keeps every migration gate false and proves no active runtime import or transport exists', () => {
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);
    expect(WOO_V3_ACCOUNT_WRITE_PARITY).toEqual(
      expect.objectContaining({
        runtimeImported: false,
        credentialedDemoLifecycle: false,
        readbackRequired: true,
        leverageReadback: 'strict-one-way-both-or-hedge-long-short',
        defaultMarginMode: 'excluded-official-response-unusable',
        positionModePreconditions: 'complete-reconciled-v3-positions-zero-and-pending-orders-empty',
        crossHedgeLeverage: 'blocked-resulting-leg-scope-requires-demo-evidence',
        isolatedLeverage: 'blocked-outside-safe-cex-cross-margin-contract',
      }),
    );
    const wooDirectory = join(__dirname, '../reference/woo');
    for (const runtimeFile of ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-private.ts', 'woo.ws-public.ts']) {
      expect(readFileSync(join(wooDirectory, runtimeFile), 'utf8')).not.toContain('woo.v3.account-writes');
    }
    const source = readFileSync(join(wooDirectory, 'woo.v3.account-writes.ts'), 'utf8');
    expect(source).not.toMatch(/\b(axios|fetch|WebSocket)\b/);
  });
});
