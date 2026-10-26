vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { installNativeStartupFrames } from './rust.native-startup.fixtures';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions, PlaceOrderOpts } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { isRustVenue, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const exchanges: RustEngineExchange[] = [];
const setup = (options: Partial<ExchangeOptions> = {}, name: ExchangeName = 'lighter') => {
  const commands: Array<Record<string, unknown>> = [];
  const noop = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities(name),
    start: noop,
    command: (json) => {
      const command = JSON.parse(json);
      if (command.op === 'setTickerSymbols' || command.op === 'setPollScale') return '[]';
      commands.push(command);
      return JSON.stringify([{ op: 'done', command: command.command, result: '[]' }]);
    },
    takeFrame: () => new Uint8Array(consumerFrame({
      loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
    })),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noop,
    wsMessage: noop,
    wsClose: noop, wsRouteExhausted: noop,
    httpStart: noop,
    httpChunk: noop,
    httpEnd: noop,
    httpFailed: noop,
    timer: noop,
  };
  const construct = vi.fn(function (_venue: string, _options: string, _setup?: string) {
    return engine;
  });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store);
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  const exchange = new RustEngineExchange(name, { key: '', secret: '', ...options }, store);
  exchanges.push(exchange);
  return { exchange, construct, commands };
};
const order = (extra: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'ETH',
  side: OrderSide.Buy,
  type: OrderType.Limit,
  amount: 1,
  price: 100,
  ...extra,
});
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

describe.each(['lighter', 'lighter-robinhood'] as const)('Rust %s wrapper contract', (name) => {
  const variantSetup = (options: Partial<ExchangeOptions> = {}) => setup(options, name);
  it('admits Lighter beta while leaving Robinhood disabled', () => {
    expect(isRustVenue('lighter')).toBe(true);
    expect(isRustVenue('lighter-robinhood')).toBe(false);
  });
  it.each([{}, { apiKeyIndex: '1' }, { secret: 'private' }])(
    'starts public without a complete signer tuple %j',
    async (options) => {
      const { exchange, construct } = variantSetup(options);
      expect(exchange.isPublicDataOnly).toBe(true);
      await exchange.start();
      expect(JSON.parse(construct.mock.calls[0][1]).publicDataOnly).toBe(true);
    },
  );
  it('forwards the distinct account and API key indices and fee/slippage options', async () => {
    const { exchange, construct } = variantSetup({
      secret: 'private',
      applicationId: '718895',
      apiKeyIndex: '0',
      extra: { lighterIntegratorAccountIndex: '158385', lighterStopSlippagePct: 2.5 },
    });
    expect(exchange.isPublicDataOnly).toBe(false);
    await exchange.start();
    expect(construct.mock.calls[0][0]).toBe(name);
    expect(JSON.parse(construct.mock.calls[0][1])).toMatchObject({
      applicationId: '718895',
      apiKeyIndex: '0',
      publicDataOnly: false,
      lighterIntegratorAccountIndex: '158385',
      lighterStopSlippagePct: 2.5,
    });
    const nativeOptions = JSON.parse(construct.mock.calls[0][1]);
    expect(nativeOptions).not.toHaveProperty('key');
    expect(nativeOptions).not.toHaveProperty('secret');
    expect(JSON.parse(construct.mock.calls[0][2]!)).toMatchObject({
      primary: { key: '', secret: 'private' },
    });
    expect(exchange.options.secret).toBeUndefined();
  });
  it('preserves remote-only private account reads without serializing callbacks', async () => {
    const sign = vi.fn();
    const { exchange, construct } = variantSetup({
      apiKeyIndex: '0',
      applicationId: '42',
      remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} },
    });
    expect(exchange.isPublicDataOnly).toBe(false);
    await exchange.start();
    expect(JSON.parse(construct.mock.calls[0][1])).toMatchObject({
      publicDataOnly: false,
      lighterRemoteSigner: true,
      apiKeyIndex: '0',
    });
    expect(JSON.parse(construct.mock.calls[0][1]).remoteSigner).toBeUndefined();
    expect(sign).not.toHaveBeenCalled();
  });
  it('keeps payload-only startup public and explicit public-only authoritative', async () => {
    const { exchange, construct } = variantSetup({ key: 'account', apiKeyIndex: '0', payloadSigner: vi.fn() });
    expect(exchange.isPublicDataOnly).toBe(true);
    await exchange.start();
    expect(JSON.parse(construct.mock.calls[0][1]).publicDataOnly).toBe(true);
    const explicit = variantSetup({ publicDataOnly: true, secret: 'local', apiKeyIndex: '0' });
    expect(explicit.exchange.isPublicDataOnly).toBe(true);
  });
  it('normalizes the existing TS API-key-index input format before crossing WASM', async () => {
    const { exchange, construct } = variantSetup({ secret: 'local', apiKeyIndex: ' key 3 ' });
    await exchange.start();
    expect(JSON.parse(construct.mock.calls[0][1]).apiKeyIndex).toBe('3');
  });
  it.each(['999', '-1', 'bad', '', undefined])(
    'rejects invalid API signing index %s during validation without starting',
    async (apiKeyIndex) => {
      const { exchange, construct } = variantSetup({ apiKeyIndex });
      expect(await exchange.validateAccount()).toEqual({
        error: 'Invalid API key index. Expected an integer from 0 to 255.',
      });
      expect(construct).not.toHaveBeenCalled();
    },
  );
  it('forwards legacy provider selection so it cannot silently enable local trading', async () => {
    const { exchange, construct } = variantSetup({
      secret: 'local',
      apiKeyIndex: '0',
      extra: { lighterSignerProvider: 'legacy' } as ExchangeOptions['extra'],
    });
    await exchange.start();
    expect(JSON.parse(construct.mock.calls[0][1]).lighterSignerProvider).toBe('legacy');
  });
  it('preserves exact integrator indices above the JS safe integer range', async () => {
    const { exchange, construct } = variantSetup({
      secret: 'private',
      applicationId: '718895',
      apiKeyIndex: '1',
      extra: { lighterIntegratorAccountIndex: '9007199254740993' },
    });
    await exchange.start();
    expect(JSON.parse(construct.mock.calls[0][1]).lighterIntegratorAccountIndex).toBe('9007199254740993');
  });
  it.each([OrderType.Limit, OrderType.Market, OrderType.StopLoss, OrderType.TakeProfit, OrderType.Conditional])(
    'rejects caller IDs on %s before any host command',
    async (type) => {
      const { exchange, commands } = variantSetup();
      await exchange.start();
      await expect(exchange.placeOrder(order({ type, clientOrderId: 'caller' }))).rejects.toThrow();
      expect(commands).toEqual([]);
    },
  );
  it('rejects the whole batch and never mints an apparently supported caller ID', async () => {
    const { exchange, commands } = variantSetup();
    await exchange.start();
    expect(exchange.mintClientOrderId(order())).toBeUndefined();
    await expect(exchange.placeOrders([order(), order({ clientOrderId: 'caller', stopLoss: 90 })])).rejects.toThrow();
    expect(commands).toEqual([]);
  });
  it('keeps caller slippage and normalizes execution time ranges to integer milliseconds', async () => {
    const { exchange, commands } = variantSetup();
    await exchange.start();
    await exchange.placeOrder(order({ maxSlippagePct: 3 }));
    await exchange.fetchHistoricalFills({ symbol: 'ETH', from: 1700000000.123, to: 1700000001456.9 });
    expect(commands[0]).toMatchObject({ op: 'placeOrders', orders: [expect.objectContaining({ maxSlippagePct: 3 })] });
    expect(commands[1]).toMatchObject({ op: 'fetchFills', from: 1700000000123, to: 1700000001456 });
  });
});
