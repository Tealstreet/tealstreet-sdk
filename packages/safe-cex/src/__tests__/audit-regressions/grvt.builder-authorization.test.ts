import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { GrvtBuilderCheckResult } from '../../exchanges/grvt/grvt.builder';

import {
  checkGrvtBuilderAuthorized,
  TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET,
  TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET,
} from '../../exchanges/grvt/grvt.builder';
import { directTransport } from '../../transport/direct';
import { setDefaultTransport } from '../../transport/registry';

const builder = TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET;
const fullRow = { builder_account_id: builder.toUpperCase(), max_futures_fee_rate: '0.03' };
const approved = { authorized: true, maxFeeRate: '0.0300%', maxFeeRateRaw: 300 };
const fetchMock = vi.fn<typeof fetch>();
const resolveHeaders = vi.fn<() => Promise<Record<string, string>>>();
const walletRequest = vi.fn(() => { throw new Error('Unexpected wallet request'); });
let previousEthereum: PropertyDescriptor | undefined;

function authResponse(accountId = 'account-id', session = 'session-cookie') {
  return new Response('{}', {
    headers: { 'x-grvt-account-id': accountId, 'x-ts-grvt-session': session },
  });
}

function respond(body: unknown) {
  fetchMock.mockResolvedValueOnce(authResponse()).mockResolvedValueOnce(new Response(JSON.stringify(body)));
}

beforeEach(() => {
  vi.useFakeTimers();
  fetchMock.mockReset();
  resolveHeaders.mockReset().mockResolvedValue({ 'X-Proxy': 'proxy-header' });
  walletRequest.mockClear();
  vi.stubGlobal('fetch', fetchMock);
  previousEthereum = Object.getOwnPropertyDescriptor(window, 'ethereum');
  Object.defineProperty(window, 'ethereum', { configurable: true, value: { request: walletRequest } });
  setDefaultTransport(directTransport);
  vi.spyOn(directTransport, 'privateProxy').mockReturnValue({ url: 'https://proxy.example', resolveHeaders });
});

afterEach(() => {
  expect(walletRequest).not.toHaveBeenCalled();
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  if (previousEthereum) Object.defineProperty(window, 'ethereum', previousEthereum);
  else Reflect.deleteProperty(window, 'ethereum');
  vi.restoreAllMocks();
});

describe('GRVT builder authorization response', () => {
  const cases: Array<{ name: string; body: unknown; expected: GrvtBuilderCheckResult }> = [
    { name: 'full fields and case-insensitive address', body: { results: [fullRow] }, expected: approved },
    { name: 'alternate full fee field', body: { results: [{ builder_account_id: builder, max_fee_rate: '0.03' }] }, expected: approved },
    { name: 'lite fields', body: { r: [{ ba: builder, mfr: '0.03' }] }, expected: approved },
    { name: 'falsy full address falls back to lite', body: { results: [{ ...fullRow, builder_account_id: '', ba: builder }] }, expected: approved },
    { name: 'empty full results wins over lite', body: { results: [], r: [fullRow] }, expected: { authorized: false } },
    { name: 'malformed full results cannot fall back to lite', body: { results: false, r: [fullRow] }, expected: { authorized: false } },
    { name: 'missing envelope fields', body: {}, expected: { authorized: false } },
    { name: 'null envelope', body: null, expected: { authorized: false } },
    { name: 'primitive envelope', body: 7, expected: { authorized: false } },
    { name: 'non-iterable results', body: { results: {} }, expected: { authorized: false } },
    { name: 'string results', body: { results: 'invalid' }, expected: { authorized: false } },
    { name: 'different address', body: { results: [{ ...fullRow, builder_account_id: 'another-builder' }] }, expected: { authorized: false } },
    { name: 'legacy builder_address is not an account-id field', body: { results: [{ builder_address: builder }] }, expected: { authorized: false } },
    { name: 'null row aborts before a later match', body: { results: [null, fullRow] }, expected: { authorized: false } },
    { name: 'invalid address aborts before a later match', body: { results: [{ builder_account_id: 7 }, fullRow] }, expected: { authorized: false } },
    { name: 'invalid full address does not fall back to valid lite', body: { results: [{ builder_account_id: {}, ba: builder }, fullRow] }, expected: { authorized: false } },
    { name: 'primitive and array rows refuse before a later match', body: { results: [7, true, '', [], fullRow] }, expected: { authorized: false } },
    { name: 'malformed row refuses after an earlier match', body: { results: [fullRow, null] }, expected: { authorized: false } },
    { name: 'zero fee is present and beats alternate fields', body: { results: [{ ...fullRow, max_futures_fee_rate: 0, max_fee_rate: '1' }] }, expected: { authorized: true, maxFeeRate: '0.0000%', maxFeeRateRaw: 0 } },
    { name: 'null fee falls through to alternate field', body: { results: [{ ...fullRow, max_futures_fee_rate: null, max_fee_rate: '0.03' }] }, expected: approved },
    { name: 'fee rounding preserves percentage units', body: { results: [{ ...fullRow, max_futures_fee_rate: '0.03006' }] }, expected: { authorized: true, maxFeeRate: '0.0301%', maxFeeRateRaw: 301 } },
    { name: 'existing numeric-prefix fee conversion', body: { results: [{ ...fullRow, max_futures_fee_rate: '0.03tail' }] }, expected: approved },
    { name: 'missing fee does not erase authorization', body: { results: [{ builder_account_id: builder }] }, expected: { authorized: true, maxFeeRate: undefined, maxFeeRateRaw: undefined } },
    { name: 'nonfinite fee does not invent a cap', body: { results: [{ ...fullRow, max_futures_fee_rate: 'Infinity' }] }, expected: { authorized: true, maxFeeRate: undefined, maxFeeRateRaw: undefined } },
  ];

  it.each(cases)('$name', async ({ body, expected }) => {
    respond(body);
    expect(await checkGrvtBuilderAuthorized('test-api-key', 'unused-subaccount')).toEqual(expected);
  });

  const malformedCases: Array<{ name: string; body: unknown; requestedBuilder?: string }> = [
    { name: 'number array with an empty requested address', body: { results: [7] }, requestedBuilder: '' },
    { name: 'number row before a matching row', body: { results: [7, fullRow] } },
    { name: 'string results with an empty requested address', body: { results: 'legacy' }, requestedBuilder: '' },
    { name: 'null results', body: { results: null } },
    { name: 'null results with a matching lite fallback', body: { results: null, r: [fullRow] } },
    { name: 'object results', body: { results: {} } },
    { name: 'object results with a matching lite fallback', body: { results: {}, r: [fullRow] } },
    { name: 'missing row address with an empty requested address', body: { results: [{}] }, requestedBuilder: '' },
    { name: 'missing row address before a matching row', body: { results: [{}, fullRow] } },
    { name: 'missing row address after a matching row', body: { results: [fullRow, {}] } },
    { name: 'boolean row before a matching row', body: { results: [false, fullRow] } },
    { name: 'string row before a matching row', body: { results: ['legacy', fullRow] } },
    { name: 'array row before a matching row', body: { results: [[], fullRow] } },
    { name: 'null row before a matching row', body: { results: [null, fullRow] } },
    { name: 'null address before a matching row', body: { results: [{ builder_account_id: null }, fullRow] } },
    { name: 'numeric address before a matching row', body: { results: [{ builder_account_id: 0 }, fullRow] } },
    { name: 'boolean address before a matching row', body: { results: [{ builder_account_id: false }, fullRow] } },
    { name: 'object address before a matching row', body: { results: [{ builder_account_id: {} }, fullRow] } },
    { name: 'mistyped lite address before a matching row', body: { r: [{ ba: 0 }, fullRow] } },
    { name: 'mistyped address after a matching row', body: { results: [fullRow, { ba: 7 }] } },
    { name: 'empty requested and returned addresses', body: { results: [{ builder_account_id: '', ba: '' }] }, requestedBuilder: '' },
    { name: 'empty requested address with a valid row', body: { results: [fullRow] }, requestedBuilder: '' },
  ];

  it.each(malformedCases)('refuses $name', async ({ body, requestedBuilder }) => {
    respond(body);
    expect(await checkGrvtBuilderAuthorized('test-api-key', '', requestedBuilder)).toEqual({ authorized: false });
  });

  it('keeps the first of two valid matching rows', async () => {
    respond({ results: [fullRow, { ...fullRow, max_futures_fee_rate: '0.05' }] });
    expect(await checkGrvtBuilderAuthorized('test-api-key', '')).toEqual(approved);
  });

  it('keeps testnet selection, authentication order, session headers and empty main-account request', async () => {
    respond({ r: [{ ba: TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET, mfr: '0.03' }] });
    expect(await checkGrvtBuilderAuthorized('test-api-key', 'ignored', undefined, true)).toEqual(approved);
    expect(resolveHeaders).toHaveBeenCalledTimes(1);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[0]).toEqual([
      'https://proxy.example/https://edge.testnet.grvt.io/auth/api_key/login',
      expect.objectContaining({ method: 'POST', credentials: 'include', body: '{"api_key":"test-api-key"}',
        headers: { 'Content-Type': 'application/json', 'X-Proxy': 'proxy-header', 'X-Grvt-Remember-Me': 'true' },
        signal: expect.any(AbortSignal) }),
    ]);
    expect(fetchMock.mock.calls[1]).toEqual([
      'https://proxy.example/https://trades.testnet.grvt.io/full/v1/get_authorized_builders',
      expect.objectContaining({ method: 'POST', body: '{}',
        headers: { 'Content-Type': 'application/json', 'X-Proxy': 'proxy-header', 'X-Grvt-Account-Id': 'account-id', 'X-Ts-Grvt-Session': 'session-cookie' },
        signal: expect.any(AbortSignal) }),
    ]);
  });

  it('keeps a missing session header absent', async () => {
    fetchMock.mockResolvedValueOnce(authResponse('account-id', '')).mockResolvedValueOnce(new Response('{"results":[]}'));
    expect(await checkGrvtBuilderAuthorized('test-api-key', '')).toEqual({ authorized: false });
    expect(fetchMock.mock.calls[1]?.[1]?.headers).not.toHaveProperty('X-Ts-Grvt-Session');
  });

  it.each(['authentication HTTP failure', 'missing account ID', 'query HTTP failure', 'query JSON failure', 'query network failure'])(
    'preserves the not-authorized result for %s', async (failure) => {
      if (failure === 'authentication HTTP failure') fetchMock.mockResolvedValueOnce(new Response('{}', { status: 401 }));
      else if (failure === 'missing account ID') fetchMock.mockResolvedValueOnce(authResponse('', ''));
      else {
        fetchMock.mockResolvedValueOnce(authResponse());
        if (failure === 'query HTTP failure') fetchMock.mockResolvedValueOnce(new Response('{}', { status: 503 }));
        else if (failure === 'query JSON failure') fetchMock.mockResolvedValueOnce(new Response('{'));
        else fetchMock.mockRejectedValueOnce(new Error('Offline'));
      }
      expect(await checkGrvtBuilderAuthorized('test-api-key', '')).toEqual({ authorized: false });
      expect(fetchMock).toHaveBeenCalledTimes(failure === 'missing account ID' || failure === 'authentication HTTP failure' ? 1 : 2);
    },
  );

  it('propagates proxy authentication failure before any request', async () => {
    const error = new Error('Proxy unavailable');
    resolveHeaders.mockRejectedValueOnce(error);
    await expect(checkGrvtBuilderAuthorized('test-api-key', '')).rejects.toBe(error);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
