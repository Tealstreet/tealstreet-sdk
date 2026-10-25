import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

const SENTINEL = 'fixture-unused-secondary-material';
const setup = (options: Partial<ExchangeOptions>) => {
  const loaded = { markets: true, tickers: true, balance: true, orders: true, positions: true };
  const noActions = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('cryptocom'),
    start: noActions,
    command: noActions,
    takeFrame: () => new Uint8Array(consumerFrame({ loaded })),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noActions,
    wsMessage: noActions,
    wsClose: noActions, wsRouteExhausted: noActions,
    httpStart: noActions,
    httpChunk: noActions,
    httpEnd: noActions,
    httpFailed: noActions,
    timer: noActions,
  };
  const construct = vi.fn(function (_venue: string, _options: string, _setup?: string) { return engine; });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store, () => ({ loaded }));
  const input = { key: 'fixture-primary-key', secret: 'fixture-primary-material', ...options };
  const exchange = new RustEngineExchange('cryptocom', input, store);
  return { construct, exchange, input };
};

describe('Crypto.com trusted provider construction', () => {
  const exchanges: RustEngineExchange[] = [];
  afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

  it.each([false, true])('consumes an unused read key without transmitting it (publicOnly=%s)', async (publicDataOnly) => {
    const { construct, exchange, input } = setup({
      publicDataOnly, secondary_key: 'fixture-unused-secondary-key', secondary_secret: SENTINEL,
    });
    exchanges.push(exchange);
    expect(JSON.stringify(exchange.options)).not.toContain(SENTINEL);
    for (const field of ['key', 'secret', 'secondary_key', 'secondary_secret'])
      expect(exchange.options).not.toHaveProperty(field);
    await exchange.start();
    expect(construct).toHaveBeenCalledOnce();
    const [venue, json, capsule] = construct.mock.calls[0];
    expect(venue).toBe('cryptocom');
    expect(JSON.parse(json)).toMatchObject({ readCredentialsValid: true, publicDataOnly });
    for (const field of ['key', 'secret', 'readCredentials']) expect(JSON.parse(json)).not.toHaveProperty(field);
    expect(JSON.parse(capsule!)).toEqual({ primary: { key: input.key, secret: input.secret } });
    expect(json + capsule).not.toContain(SENTINEL);
    expect(json + capsule).not.toContain('fixture-unused-secondary-key');
    expect(input.secondary_secret).toBe(SENTINEL);
  });

  it('carries a bad read key as false for native construction refusal without leaking its material', async () => {
    const { construct, exchange } = setup({ secondary_key: SENTINEL, secondary_secret: '' });
    exchanges.push(exchange);
    await exchange.start();
    expect(construct).toHaveBeenCalledOnce();
    const [, json, capsule] = construct.mock.calls[0];
    expect(JSON.parse(json)).toHaveProperty('readCredentialsValid', false);
    expect(JSON.parse(capsule!)).not.toHaveProperty('secondary');
    expect(json + capsule).not.toContain(SENTINEL);
  });

  it('omits validation metadata and the secondary capsule when no read key exists', async () => {
    const { construct, exchange } = setup({});
    exchanges.push(exchange);
    await exchange.start();
    const [, json, capsule] = construct.mock.calls[0];
    expect(JSON.parse(json)).not.toHaveProperty('readCredentialsValid');
    expect(JSON.parse(capsule!)).not.toHaveProperty('secondary');
  });
});
