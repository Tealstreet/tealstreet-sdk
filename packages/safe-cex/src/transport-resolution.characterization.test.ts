import type { ExchangeAccountConfig, ExchangeName, ExchangeOptions } from './types';

import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import './transport/registerTealstreetTransport';

import { accountToExchangeOptions } from './createExchangeFromAccount';
import { exchangeMetadata } from './exchangeMetadata';
import { getProxyForAxios } from './transport/tealstreet/tealstreetTransport';

// Golden-master characterization of the proxy / tunnel / CORS transport
// resolution. It locks CURRENT behavior across EVERY exchange and EVERY
// permutation of the user transport preferences, on purpose, BEFORE the planned
// hoist of this logic into a single source of truth — so the refactor can be
// proven behavior-preserving (snapshot must not move). It uses the REAL
// exchangeMetadata (not a mock) so a metadata change that shifts any exchange's
// routing surfaces here. Intentional behavior changes (e.g. the secondary-key
// gate) update this snapshot deliberately.

const ALL_EXCHANGES = Object.keys(exchangeMetadata) as ExchangeName[];
const BOOL = [false, true];
const REGIONS: Array<string | undefined> = [undefined, 'JA', 'SG'];

// Compact, faithful one-line encoding of the resolved options that drive transport.
const encodeOptions = (o: ExchangeOptions): string => {
  const proxy = o.tealstreetProxy ? 'proxy' : '-';
  const tunnel = o.tealstreetTunnel ? 'tunnel' : '-';
  const region = o.proxyRegion ?? '-';
  // secProxy was `exchange-proxy.jp` (AWS) until PUBLIC_CORS_PROXY moved to the edge
  // Worker's /api/public_proxy (AWS_EXIT.md 3d); that path is not shortened, so the
  // snapshot now pins the full `fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy`.
  const sec = o.secondaryProxyUrl
    ? o.secondaryProxyUrl.replace('https://', '').replace('.tealstreet.io/api/exchange_proxy', '')
    : '-';
  return `${proxy} | ${tunnel} | region=${region} | secProxy=${sec}`;
};

describe('transport resolution (golden master)', () => {
  let warnSpy: ReturnType<typeof vi.spyOn>;
  beforeAll(() => {
    warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
  });
  afterAll(() => warnSpy.mockRestore());

  it('accountToExchangeOptions: every exchange x every preference permutation', () => {
    const matrix: Record<string, Record<string, string>> = {};
    for (const exchange of ALL_EXCHANGES) {
      const perExchange: Record<string, string> = {};
      for (const tealstreetProxy of BOOL) {
        for (const tealstreetTunnel of BOOL) {
          for (const secondary of BOOL) {
            for (const proxyRegion of REGIONS) {
              const account: ExchangeAccountConfig = {
                exchange,
                name: 'test',
                apiKey: 'key',
                apiSecret: 'secret',
                tealstreetProxy,
                tealstreetTunnel,
                ...(proxyRegion ? { proxyRegion } : {}),
                ...(secondary ? { secondary_apiKey: 'rk', secondary_apiSecret: 'rs' } : {}),
              };
              const key = `proxy=${Number(tealstreetProxy)} tunnel=${Number(tealstreetTunnel)} sec=${Number(
                secondary,
              )} region=${proxyRegion ?? 'NONE'}`;
              perExchange[key] = encodeOptions(accountToExchangeOptions(account));
            }
          }
        }
      }
      matrix[exchange] = perExchange;
    }
    expect(matrix).toMatchSnapshot();
  });

  it('forwards Robinhood Lighter account index and API key index', () => {
    const options = accountToExchangeOptions({
      exchange: 'lighter-robinhood',
      name: 'test-robinhood-lighter',
      apiKey: 'key',
      apiSecret: 'secret',
      applicationId: '718895',
      apiKeyIndex: '2',
    });

    expect(options.applicationId).toBe('718895');
    expect(options.apiKeyIndex).toBe('2');
  });

  it('getProxyForAxios: representative base URLs x proxy/region/auth permutations', () => {
    const URLS: Array<[string, ExchangeName]> = [
      ['https://api.bybit.com', 'bybit'],
      ['https://fapi.binance.com', 'binance'],
      ['https://www.okx.com/api', 'okx'],
      ['https://api.bitget.com', 'bitget'],
      ['https://grvt.io/api', 'grvt'],
      ['https://api.hyperliquid.xyz', 'hyperliquid'],
    ];
    const matrix: Record<string, Record<string, string>> = {};
    for (const [baseURL, exchange] of URLS) {
      const perUrl: Record<string, string> = {};
      for (const tealstreetProxy of BOOL) {
        for (const proxyRegion of REGIONS) {
          for (const authed of BOOL) {
            const config = { baseURL, headers: authed ? { 'X-MBX-APIKEY': 'k' } : {} };
            const options = {
              tealstreetProxy,
              ...(proxyRegion ? { proxyRegion } : {}),
              ...(authed ? {} : { publicDataOnly: true }),
            } as ExchangeOptions;
            const key = `proxy=${Number(tealstreetProxy)} region=${proxyRegion ?? 'NONE'} authed=${Number(authed)}`;
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            perUrl[key] = getProxyForAxios(config as any, options, exchange) || 'DIRECT';
          }
        }
      }
      matrix[baseURL] = perUrl;
    }
    expect(matrix).toMatchSnapshot();
  });
});
