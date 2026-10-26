import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  KUCOIN_UTA_ACCOUNT_MODES,
  KUCOIN_UTA_ACCOUNT_MODE_ROUTE,
  KUCOIN_UTA_ACCOUNT_MODE_SUCCESS_CODE,
  KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED,
  decideKucoinClassicRuntime,
  decodeKucoinAccountMode,
} from '../reference/kucoin/kucoin.uta-account-mode';

const response = (
  selfAccountMode: unknown,
  unifiedSubAccount: unknown = [],
  classicSubAccount: unknown = [],
): unknown => ({
  code: '200000',
  data: { selfAccountMode, unifiedSubAccount, classicSubAccount },
});

const officialClassicResponse = (): unknown => response('CLASSIC', [], [252118980]);
const officialUnifiedResponse = (): unknown => response('UNIFIED', [169579801], [215442919, 225139445]);

describe('KuCoin inert UTA account-mode contract', () => {
  it('locks the published route, success code, and complete account-mode enum', () => {
    expect(KUCOIN_UTA_ACCOUNT_MODE_ROUTE).toEqual({
      baseUrl: 'https://api.kucoin.com',
      method: 'GET',
      path: '/api/ua/v1/account/mode',
    });
    expect(KUCOIN_UTA_ACCOUNT_MODE_SUCCESS_CODE).toBe('200000');
    expect(KUCOIN_UTA_ACCOUNT_MODES).toEqual(['CLASSIC', 'UNIFIED']);
    expect(KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED).toBe(false);
  });

  it('allows the existing runtime only after a positive CLASSIC identification', () => {
    const detection = decodeKucoinAccountMode(officialClassicResponse());

    expect(detection).toEqual({
      status: 'identified',
      accountMode: 'CLASSIC',
      unifiedSubAccount: [],
      classicSubAccount: [252118980],
    });
    expect(decideKucoinClassicRuntime(detection)).toEqual({
      eligible: true,
      reason: 'classic-account-mode-confirmed',
      route: 'existing-classic-runtime',
      utaRuntimeActivationEnabled: false,
    });
  });

  it('identifies UNIFIED but never routes it into the Classic integration', () => {
    const detection = decodeKucoinAccountMode(officialUnifiedResponse());

    expect(detection).toEqual({
      status: 'identified',
      accountMode: 'UNIFIED',
      unifiedSubAccount: [169579801],
      classicSubAccount: [215442919, 225139445],
    });
    expect(decideKucoinClassicRuntime(detection)).toEqual({
      eligible: false,
      reason: 'unified-account-mode-unsupported',
      route: 'blocked',
      utaRuntimeActivationEnabled: false,
    });
  });

  it.each(['classic', 'UNIFIED ', 'OPTIONS', ''])('never broadens an unpublished mode value %j', (mode) => {
    const detection = decodeKucoinAccountMode(response(mode));

    if (mode === '') {
      expect(detection).toEqual({ status: 'malformed', reason: 'invalid-self-account-mode' });
    } else {
      expect(detection).toEqual({ status: 'unknown', reason: 'unknown-self-account-mode' });
    }
    expect(decideKucoinClassicRuntime(detection)).toMatchObject({
      eligible: false,
      route: 'blocked',
      utaRuntimeActivationEnabled: false,
    });
  });

  it.each([
    [null, 'invalid-envelope'],
    [[], 'invalid-envelope'],
    [
      { code: 200000, data: { selfAccountMode: 'CLASSIC', unifiedSubAccount: [], classicSubAccount: [] } },
      'invalid-success-code',
    ],
    [
      { code: '400100', data: { selfAccountMode: 'CLASSIC', unifiedSubAccount: [], classicSubAccount: [] } },
      'invalid-success-code',
    ],
    [{ code: '200000' }, 'invalid-envelope'],
    [{ code: '200000', data: [] }, 'invalid-data'],
    [{ code: '200000', data: {} }, 'invalid-data'],
    [
      { code: '200000', data: { selfAccountMode: null, unifiedSubAccount: [], classicSubAccount: [] } },
      'invalid-self-account-mode',
    ],
  ])('fails closed for malformed response %#', (payload, reason) => {
    const detection = decodeKucoinAccountMode(payload);

    expect(detection).toEqual({ status: 'malformed', reason });
    expect(decideKucoinClassicRuntime(detection)).toEqual({
      eligible: false,
      reason: 'account-mode-response-malformed',
      route: 'blocked',
      utaRuntimeActivationEnabled: false,
    });
  });

  it('accepts exact own-key schemas with either a plain or null prototype', () => {
    const data = Object.assign(Object.create(null) as Record<string, unknown>, {
      selfAccountMode: 'CLASSIC',
      unifiedSubAccount: [],
      classicSubAccount: [252118980],
    });
    const envelope = Object.assign(Object.create(null) as Record<string, unknown>, {
      code: '200000',
      data,
    });

    expect(decodeKucoinAccountMode(envelope)).toEqual({
      status: 'identified',
      accountMode: 'CLASSIC',
      unifiedSubAccount: [],
      classicSubAccount: [252118980],
    });
  });

  it.each([
    ['inherited envelope keys', Object.create(response('CLASSIC') as object), 'invalid-envelope'],
    [
      'inherited data key',
      {
        code: '200000',
        data: Object.create({ selfAccountMode: 'CLASSIC', unifiedSubAccount: [], classicSubAccount: [] }),
      },
      'invalid-data',
    ],
    [
      'arbitrary envelope prototype',
      Object.assign(Object.create({ marker: true }) as Record<string, unknown>, response('CLASSIC')),
      'invalid-envelope',
    ],
    [
      'arbitrary data prototype',
      {
        code: '200000',
        data: Object.assign(Object.create({ marker: true }) as Record<string, unknown>, {
          selfAccountMode: 'CLASSIC',
          unifiedSubAccount: [],
          classicSubAccount: [],
        }),
      },
      'invalid-data',
    ],
    ['extra envelope string key', { ...(response('CLASSIC') as object), msg: 'success' }, 'invalid-envelope'],
    [
      'extra data string key',
      {
        code: '200000',
        data: { selfAccountMode: 'CLASSIC', unifiedSubAccount: [], classicSubAccount: [], unexpected: true },
      },
      'invalid-data',
    ],
    [
      'numeric success code',
      { code: 200000, data: { selfAccountMode: 'CLASSIC', unifiedSubAccount: [], classicSubAccount: [] } },
      'invalid-success-code',
    ],
    [
      'non-success error envelope',
      { code: '400100', data: { selfAccountMode: 'CLASSIC', unifiedSubAccount: [], classicSubAccount: [] } },
      'invalid-success-code',
    ],
  ])('rejects %s', (_name, payload, reason) => {
    expect(decodeKucoinAccountMode(payload)).toEqual({ status: 'malformed', reason });
  });

  it('rejects extra symbol keys at either schema level', () => {
    const envelopeSymbol = response('CLASSIC') as Record<PropertyKey, unknown>;
    envelopeSymbol[Symbol('extra')] = true;
    expect(decodeKucoinAccountMode(envelopeSymbol)).toEqual({ status: 'malformed', reason: 'invalid-envelope' });

    const dataSymbol = response('CLASSIC') as { code: string; data: Record<PropertyKey, unknown> };
    dataSymbol.data[Symbol('extra')] = true;
    expect(decodeKucoinAccountMode(dataSymbol)).toEqual({ status: 'malformed', reason: 'invalid-data' });
  });

  it('rejects non-enumerable required or extra keys', () => {
    const hiddenEnvelopeKey = response('CLASSIC') as Record<string, unknown>;
    Object.defineProperty(hiddenEnvelopeKey, 'hidden', { value: true, enumerable: false });
    expect(decodeKucoinAccountMode(hiddenEnvelopeKey)).toEqual({ status: 'malformed', reason: 'invalid-envelope' });

    const hiddenDataKey = response('CLASSIC') as { code: string; data: Record<string, unknown> };
    Object.defineProperty(hiddenDataKey.data, 'hidden', { value: true, enumerable: false });
    expect(decodeKucoinAccountMode(hiddenDataKey)).toEqual({ status: 'malformed', reason: 'invalid-data' });

    const nonEnumerableMode = {
      code: '200000',
      data: { unifiedSubAccount: [], classicSubAccount: [] } as Record<string, unknown>,
    };
    Object.defineProperty(nonEnumerableMode.data, 'selfAccountMode', { value: 'CLASSIC', enumerable: false });
    expect(decodeKucoinAccountMode(nonEnumerableMode)).toEqual({ status: 'malformed', reason: 'invalid-data' });
  });

  it('rejects accessor-backed fields without invoking them', () => {
    let invoked = false;
    const data = { unifiedSubAccount: [], classicSubAccount: [] } as Record<string, unknown>;
    Object.defineProperty(data, 'selfAccountMode', {
      enumerable: true,
      get: () => {
        invoked = true;
        return 'CLASSIC';
      },
    });

    expect(decodeKucoinAccountMode({ code: '200000', data })).toEqual({
      status: 'malformed',
      reason: 'invalid-data',
    });
    expect(invoked).toBe(false);
  });

  it.each([
    ['sparse', new Array(1)],
    ['fractional id', [1.5]],
    ['zero id', [0]],
    ['negative id', [-1]],
    ['unsafe id', [Number.MAX_SAFE_INTEGER + 1]],
    ['NaN id', [Number.NaN]],
    ['infinite id', [Number.POSITIVE_INFINITY]],
    ['string id', ['252118980']],
  ])('rejects a %s subaccount array', (_name, ids) => {
    expect(decodeKucoinAccountMode(response('CLASSIC', ids, []))).toEqual({
      status: 'malformed',
      reason: 'invalid-data',
    });
    expect(decodeKucoinAccountMode(response('CLASSIC', [], ids))).toEqual({
      status: 'malformed',
      reason: 'invalid-data',
    });
  });

  it('rejects accessor, non-enumerable index, extra-property, symbol, and non-plain subaccount arrays', () => {
    const accessorArray: unknown[] = [];
    Object.defineProperty(accessorArray, '0', { enumerable: true, get: () => 252118980 });

    const nonEnumerableIndex = new Array(1);
    Object.defineProperty(nonEnumerableIndex, '0', { enumerable: false, value: 252118980 });

    const extraProperty = [252118980] as unknown[] & { extra?: boolean };
    extraProperty.extra = true;

    const symbolProperty = [252118980] as unknown[] & Record<PropertyKey, unknown>;
    symbolProperty[Symbol('extra')] = true;

    const customPrototype = [252118980];
    Object.setPrototypeOf(customPrototype, Object.create(Array.prototype));

    for (const ids of [accessorArray, nonEnumerableIndex, extraProperty, symbolProperty, customPrototype]) {
      expect(decodeKucoinAccountMode(response('CLASSIC', ids, []))).toEqual({
        status: 'malformed',
        reason: 'invalid-data',
      });
    }
  });

  it('returns frozen ID provenance detached from later input mutations', () => {
    const unifiedSubAccount = [169579801];
    const classicSubAccount = [215442919, 225139445];
    const detection = decodeKucoinAccountMode(response('UNIFIED', unifiedSubAccount, classicSubAccount));

    expect(detection.status).toBe('identified');
    if (detection.status !== 'identified') throw new Error('expected identified account mode');
    unifiedSubAccount[0] = 1;
    classicSubAccount.push(1);

    expect(detection.unifiedSubAccount).toEqual([169579801]);
    expect(detection.classicSubAccount).toEqual([215442919, 225139445]);
    expect(Object.isFrozen(detection)).toBe(true);
    expect(Object.isFrozen(detection.unifiedSubAccount)).toBe(true);
    expect(Object.isFrozen(detection.classicSubAccount)).toBe(true);
  });

  it('fails closed when reflection over an exotic response throws', () => {
    const payload = new Proxy(
      {},
      {
        getPrototypeOf: () => {
          throw new Error('untrusted proxy');
        },
      },
    );

    expect(decodeKucoinAccountMode(payload)).toEqual({ status: 'malformed', reason: 'invalid-envelope' });
  });

  it('remains absent from every active KuCoin runtime and API factory', () => {
    for (const file of [
      'kucoin.api.ts',
      'kucoin.exchange.ts',
      'kucoin.ws-private.ts',
      'kucoin.ws-public.ts',
      'kucoinspot.api.ts',
      'kucoinspot.exchange.ts',
      'kucoinspot.ws-private.ts',
      'kucoinspot.ws-public.ts',
    ]) {
      const source = readFileSync(resolve(__dirname, '../reference/kucoin', file), 'utf8');
      expect(source).not.toContain('kucoin.uta-account-mode');
    }
  });
});
