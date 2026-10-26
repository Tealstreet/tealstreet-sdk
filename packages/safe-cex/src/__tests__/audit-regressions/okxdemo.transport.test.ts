import type { ExchangeOptions } from '../../types';

import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { BASE_URLS, BASE_WS_URLS } from '../../exchanges/okx/okx.types';
import { OKXDemoExchange } from '../reference/okx/okxdemo.exchange';
import { OKXEUDemoExchange } from '../reference/okx/okxeudemo.exchange';
import { exchangeRegistry } from '../../exchanges/registry';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { resolveExchangeRouting } from '../../transport/tealstreet/routing/resolveExchangeRouting';

const options: ExchangeOptions = {
  key: 'test-key',
  secret: 'test-secret',
  passphrase: 'test-passphrase',
  testnet: false,
  tuning: { consoleLogging: false },
};

describe('OKX Demo transport and metadata', () => {
  it('uses default TLS 443 for every global and EEA live/demo channel', () => {
    for (const [region, liveHost, demoHost] of [
      ['okx', 'ws.okx.com', 'wspap.okx.com'],
      ['okxeu', 'wseea.okx.com', 'wseeapap.okx.com'],
    ] as const) {
      for (const channel of ['public', 'private', 'business'] as const) {
        for (const [environment, host] of [
          ['livenet', liveHost],
          ['testnet', demoHost],
        ] as const) {
          const endpoint = BASE_WS_URLS[region][channel][environment];
          expect(endpoint).toBe(`wss://${host}/ws/v5/${channel}`);
          const url = new URL(endpoint);
          expect(url.protocol).toBe('wss:');
          expect(url.port || '443').toBe('443');
        }
      }
    }
  });

  it('uses the OKX runtime with demo mode forced on', () => {
    const exchange = new OKXDemoExchange('okxdemo', options, new DefaultStore());

    expect(exchange.isDemo).toBe(true);
    expect(exchange.options.testnet).toBe(true);
    expect(exchange.region).toBe('okx');

    exchange.dispose();
  });

  it('uses official direct OKX demo websocket endpoints', () => {
    expect(BASE_URLS.okx.testnet).toBe('https://openapi.okx.com');
    expect(BASE_WS_URLS.okx.public.testnet).toBe('wss://wspap.okx.com/ws/v5/public');
    expect(BASE_WS_URLS.okx.private.testnet).toBe('wss://wspap.okx.com/ws/v5/private');
    expect(BASE_WS_URLS.okx.business.testnet).toBe('wss://wspap.okx.com/ws/v5/business');
  });

  it('uses the OKX EU runtime with EEA demo mode forced on', () => {
    const exchange = new OKXEUDemoExchange('okxeudemo', options, new DefaultStore());

    expect(exchange.isDemo).toBe(true);
    expect(exchange.options.testnet).toBe(true);
    expect(exchange.region).toBe('okxeu');

    exchange.dispose();
  });

  it('uses official direct OKX EU demo websocket endpoints', () => {
    expect(BASE_URLS.okxeu.testnet).toBe('https://eea.okx.com');
    expect(BASE_WS_URLS.okxeu.public.testnet).toBe('wss://wseeapap.okx.com/ws/v5/public');
    expect(BASE_WS_URLS.okxeu.private.testnet).toBe('wss://wseeapap.okx.com/ws/v5/private');
    expect(BASE_WS_URLS.okxeu.business.testnet).toBe('wss://wseeapap.okx.com/ws/v5/business');
  });

  it('is registered as a REST-proxied, WS-direct separate exchange variant', async () => {
    await expect(exchangeRegistry.okxdemo()).resolves.toBe(RustEngineExchange);

    expect(exchangeMetadata.okxdemo).toMatchObject({
      exchangeName: 'okxdemo',
      prettyName: 'OKX Demo',
      supportsTestnet: false,
      requiresCors: true,
      forbidCors: false,
      enforceWsTunnel: false,
      latencyDomain: 'openapi.okx.com',
    });

    expect(resolveExchangeRouting('okxdemo', { tealstreetProxy: true }).restProxyHost).toBe(
      'https://fsjnpy143pbz.jp.tealstreet.io/api/exchange_proxy',
    );
    expect(resolveExchangeRouting('okxdemo', { tealstreetProxy: true }).wsPrivate).toEqual({ tunnelled: false });
    expect(resolveExchangeRouting('okxdemo', { tealstreetProxy: true }).wsBusiness).toEqual({ tunnelled: false });
  });

  it('registers OKX EU Demo as REST-proxied and WS-direct', async () => {
    await expect(exchangeRegistry.okxeudemo()).resolves.toBe(RustEngineExchange);

    expect(exchangeMetadata.okxeudemo).toMatchObject({
      exchangeName: 'okxeudemo',
      prettyName: 'OKX EU Demo',
      supportsTestnet: false,
      requiresCors: true,
      forbidCors: false,
      enforceWsTunnel: false,
      latencyDomain: 'eea.okx.com',
    });

    expect(resolveExchangeRouting('okxeudemo', { tealstreetProxy: true }).restProxyHost).toBe(
      'https://fsjnpy143pbz.jp.tealstreet.io/api/exchange_proxy',
    );
    expect(resolveExchangeRouting('okxeudemo', { tealstreetProxy: true }).wsPrivate).toEqual({ tunnelled: false });
    expect(resolveExchangeRouting('okxeudemo', { tealstreetProxy: true }).wsBusiness).toEqual({ tunnelled: false });
  });
});
