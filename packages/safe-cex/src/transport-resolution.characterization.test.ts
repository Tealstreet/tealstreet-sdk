import type { ExchangeAccountConfig, ExchangeName, ExchangeOptions } from './types';

import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { accountToExchangeOptions } from './createExchangeFromAccount';
import { exchangeMetadata } from './exchangeMetadata';
import { getProxyForAxios } from './safeCexAxiosCreate';

// Golden-master characterization of the proxy / tunnel / CORS transport
// resolution. It locks CURRENT behavior across EVERY exchange and EVERY
// permutation of the user transport preferences, on purpose, BEFORE the planned
// hoist of this logic into a single source of truth — so the refactor can be
// proven behavior-preserving (snapshot must not move). It uses the REAL
// exchangeMetadata (not a mock) so a metadata change that shifts any exchange's
// routing surfaces here. Intentional behavior changes (the AWS->Vultr CORS
// migration, the secondary-key gate) update this snapshot deliberately.

const ALL_EXCHANGES = Object.keys(exchangeMetadata) as ExchangeName[];
const BOOL = [false, true];
const REGIONS: Array<string | undefined> = [undefined, 'JA', 'SG'];

// Compact, faithful one-line encoding of the resolved options that drive transport.
const encodeOptions = (o: ExchangeOptions): string => {
  const proxy = o.tealstreetProxy ? 'proxy' : '-';
  const tunnel = o.tealstreetTunnel ? 'tunnel' : '-';
  const region = o.proxyRegion ?? '-';
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

  it('getProxyForAxios: representative base URLs x proxy/region/auth permutations', () => {
    const URLS = [
      'https://api.bybit.com',
      'https://fapi.binance.com',
      'https://www.okx.com/api',
      'https://api.bitget.com',
      'https://grvt.io/api',
      'https://www.bitmex.com/api',
      'https://api.hyperliquid.xyz',
    ];
    const matrix: Record<string, Record<string, string>> = {};
    for (const baseURL of URLS) {
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
            perUrl[key] = getProxyForAxios(config as any, options) || 'DIRECT';
          }
        }
      }
      matrix[baseURL] = perUrl;
    }
    expect(matrix).toMatchSnapshot();
  });
});
