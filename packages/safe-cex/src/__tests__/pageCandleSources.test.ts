import type { ExchangeOptions } from '../types';

import { expect, it } from 'vitest';

import { pageCandleSourceKey, pageCandleSourceOptions } from '../exchanges/rust/pageCandleSources';

const account = (options: Partial<ExchangeOptions>) =>
  pageCandleSourceOptions({ key: '', secret: '', workerMirror: true, ...options } as ExchangeOptions);

it('keys a page candle source by venue configuration, never by account or credential', () => {
  const first = account({
    key: 'PRIVATE_KEY',
    secret: 'PRIVATE_SECRET',
    applicationId: '0xfirst',
    vault: '0xvault',
    auth: { userAddress: '0xuser', privateKey: 'PRIVATE_SIGNER' },
    tealstreetProxy: true,
    polymarket: { marketSlugs: ['slug'], apiKey: 'PRIVATE_POLY' },
  });
  const second = account({
    key: 'OTHER',
    secret: 'OTHER',
    applicationId: '0xsecond',
    tealstreetProxy: true,
    polymarket: { marketSlugs: ['slug'] },
  });
  expect(first).toMatchObject({ publicDataOnly: true, workerMirror: false, privateOnly: false });
  const key = pageCandleSourceKey('hyperliquid', first);
  expect(key).toBe(pageCandleSourceKey('hyperliquid', second));
  expect(`${key}${JSON.stringify(first)}`).not.toMatch(/PRIVATE|0xfirst|0xvault|0xuser/);
  expect(key).not.toBe(pageCandleSourceKey('hyperliquid', account({ tealstreetProxy: false })));
  expect(key).not.toBe(pageCandleSourceKey('hyperliquidspot', first));
});

it('tells host functions and class instances apart by identity', () => {
  const provider = () => ({ restAdapter: () => undefined }) as unknown as ExchangeOptions['transport'];
  const transport = provider();
  const same = pageCandleSourceKey('hyperliquid', account({ transport }));
  expect(pageCandleSourceKey('hyperliquid', account({ transport }))).toBe(same);
  expect(pageCandleSourceKey('hyperliquid', account({ transport: provider() }))).not.toBe(same);
  class Provider {}
  const instance = new Provider() as ExchangeOptions['transport'];
  expect(pageCandleSourceKey('hyperliquid', account({ transport: instance }))).not.toBe(
    pageCandleSourceKey('hyperliquid', account({ transport: new Provider() as ExchangeOptions['transport'] })),
  );
});
