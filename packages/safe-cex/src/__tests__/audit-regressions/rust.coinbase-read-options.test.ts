// @vitest-environment node
import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { RestTransportContext } from '../../transport/types';
import type { ExchangeOptions } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';
import { LogSeverity } from '../../types';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { nativeReadyFrame } from './rust.native-startup.fixtures';

const accounts: RustEngineExchange[] = [];
const aliases = ['coinbase', 'coinbasespot'] as const;
const privateUrl = 'https://api.coinbase.com/api/v3/brokerage/accounts?limit=100';

afterEach(() => {
  accounts.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
  vi.restoreAllMocks();
});

function fixture(name: (typeof aliases)[number], saved: Partial<ExchangeOptions>) {
  const route = vi.fn((context: RestTransportContext) => ({
    baseURL: `${context.options.overrideProxyUrl}/${context.config.baseURL}`,
    proxied: true,
    authorize: async (config: Parameters<NonNullable<ReturnType<typeof directTransport.rest>['authorize']>>[0]) => {
      config.headers.set('Authorization', 'Bearer primary-proxy-token');
    },
  }));
  const wsRouting = vi.fn(directTransport.wsRouting);
  const request = vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array() });
  setRustEngineTransport(() => ({
    now: Date.now,
    request,
    connect: () => ({ send() {}, close() {} }),
    setTimeout,
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  }));
  const idle = () => '[]';
  const native: WasmEngineLike = {
    capabilities: () => engineCapabilities(name),
    // A controlled signed private action exercises the actual host and routing;
    // JWT cryptography itself remains native and is not implemented by this fixture.
    start: () =>
      JSON.stringify([
        {
          op: 'http',
          request: 1,
          method: 'GET',
          url: privateUrl,
          headers: [['Authorization', 'Bearer primary-venue-jwt']],
        },
      ]),
    command: (json) => {
      const command = JSON.parse(json);
      return command.command === undefined
        ? '[]'
        : JSON.stringify([{ op: 'done', command: command.command, result: 'null' }]);
    },
    takeFrame: nativeReadyFrame,
    snapshotFrame: () => new Uint8Array(consumerFrame({ markets: [], tickers: [] })),
    wsOpen: idle,
    wsMessage: idle,
    wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle,
    httpChunk: idle,
    httpEnd: idle,
    httpFailed: idle,
    timer: idle,
  };
  const construct = vi.fn(function (_venue: string, _options: string) {
    return native;
  });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const options: ExchangeOptions = Object.freeze({
    key: 'primary-key',
    secret: 'primary-secret',
    passphrase: 'primary-passphrase',
    overrideProxyUrl: 'https://primary.proxy',
    publicDataOnly: false,
    privateOnly: true,
    publicFeed: { mode: 'disabled' as const },
    tuning: { consoleLogging: false },
    ...saved,
    transport: { ...directTransport, rest: route, wsRouting },
  });
  const exchange = new RustEngineExchange(name, options, new DefaultStore());
  accounts.push(exchange);
  const logs = vi.fn();
  exchange.on('log', logs);
  return { exchange, options, logs, route, wsRouting, request, construct };
}

it.each(aliases)(
  '%s drops ignored saved read credentials at every boundary without changing saved options',
  async (name) => {
    const saved = {
      secondary_key: 'saved-read-key',
      secondary_secret: 'saved-read-secret',
      secondary_passphrase: 'saved-read-passphrase',
      secondaryProxyUrl: 'https://saved-read.proxy',
    };
    const r = fixture(name, saved);
    expect(r.logs).not.toHaveBeenCalled();
    expect(r.exchange.options).not.toBe(r.options);
    await r.exchange.start();
    await r.exchange.start();
    await vi.waitFor(() => expect(r.request).toHaveBeenCalledOnce());
    expect(r.options).toMatchObject(saved);
    expect(r.exchange.options).toMatchObject({
      key: 'primary-key',
      secret: 'primary-secret',
      passphrase: 'primary-passphrase',
      overrideProxyUrl: 'https://primary.proxy',
      privateOnly: true,
      publicFeed: { mode: 'disabled' },
    });
    for (const key of Object.keys(saved)) expect(r.exchange.options).not.toHaveProperty(key);
    expect(r.wsRouting.mock.calls.every(([, options]) => options === r.exchange.options)).toBe(true);
    expect(r.route.mock.calls.every(([context]) => context.options === r.exchange.options)).toBe(true);
    expect(JSON.parse(r.construct.mock.calls[0][1])).toMatchObject({
      key: 'primary-key',
      secret: 'primary-secret',
      passphrase: 'primary-passphrase',
      publicDataOnly: false,
      publicFeed: { mode: 'disabled' },
    });
    expect(JSON.parse(r.construct.mock.calls[0][1])).not.toHaveProperty('readCredentials');
    expect(r.request.mock.calls[0][0]).toMatchObject({
      method: 'GET',
      url: `https://primary.proxy/${privateUrl}`,
      headers: { 'Coinbase-Authorization': 'Bearer primary-venue-jwt', Authorization: 'Bearer primary-proxy-token' },
    });
    expect(r.logs).toHaveBeenCalledExactlyOnceWith(
      `[${name}] Saved read credentials and their proxy are ignored; Coinbase private reads use primary credentials.`,
      LogSeverity.Warning,
    );
    expect(JSON.stringify(r.logs.mock.calls)).not.toMatch(/saved-read|primary-key|primary-secret|primary-passphrase/);
  },
);

it.each(aliases)(
  '%s accepts partial and empty saved read fields without secondary routing validation',
  async (name) => {
    for (const saved of [
      { secondary_key: '' },
      { secondary_secret: '' },
      { secondary_passphrase: '' },
      { secondary_key: 'read-key' },
      { secondary_secret: 'read-secret' },
      { secondary_passphrase: 'read-passphrase' },
    ]) {
      const r = fixture(name, { ...saved, secondaryProxyUrl: 'https://unused-read.proxy' });
      await r.exchange.start();
      expect(r.construct).toHaveBeenCalledOnce();
      expect(r.exchange.options).not.toHaveProperty('secondaryProxyUrl');
      // An empty saved field is no read key: it is dropped without a warning.
      expect(r.logs).toHaveBeenCalledTimes(Object.values(saved).some(Boolean) ? 1 : 0);
      r.exchange.dispose();
    }
  },
);

it.each(aliases)('%s preserves a standalone secondary proxy when no read credentials are configured', async (name) => {
  const r = fixture(name, { secondaryProxyUrl: 'https://standalone.proxy' });
  await r.exchange.start();
  expect(r.exchange.options.secondaryProxyUrl).toBe('https://standalone.proxy');
  expect(r.logs).not.toHaveBeenCalled();
  expect(r.wsRouting.mock.calls.every(([, options]) => options.secondaryProxyUrl === 'https://standalone.proxy')).toBe(
    true,
  );
  await vi.waitFor(() => expect(r.request).toHaveBeenCalledOnce());
  expect(r.request.mock.calls[0][0].url).toBe(`https://primary.proxy/${privateUrl}`);
});
