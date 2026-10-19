import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions, Market, Order, PlaceOrderOpts } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { replacementClientOrderIdOf } from '../../exchanges/clientOrderId';
import { isRustVenue, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

const CLOID = '0x0123456789abcdef0123456789abcdef';
const MARKET: Market = {
  id: 'BTC',
  symbol: 'BTC-PERP',
  base: 'BTC',
  quote: 'USDC',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 50 } },
  info: {},
};
const ORDER: Order = {
  id: '42',
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  status: OrderStatus.Open,
  price: 100,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  clientOrderId: CLOID,
  lastUpdate: 1000,
};
const placement = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 100,
  ...overrides,
});

// Exercise the real JS host's command/result protocol without building WASM.
const fixture = (options: Partial<ExchangeOptions> = {}) => {
  const result = vi.fn((_command: Record<string, unknown>): unknown => []);
  const commands: Array<Record<string, unknown>> = [];
  const noActions = () => '[]';
  const engine: WasmEngineLike = {
    start: noActions,
    command: (json) => {
      const command = JSON.parse(json);
      commands.push(command);
      return JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(result(command)) }]);
    },
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noActions,
    wsMessage: noActions,
    wsClose: noActions,
    httpStart: noActions,
    httpChunk: noActions,
    httpEnd: noActions,
    httpFailed: noActions,
    timer: noActions,
  };
  const construct = vi.fn(function (_venue: string, _options: string) {
    return engine;
  });
  const load = vi.fn(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  setWasmEngineLoader(load);
  const store = new DefaultStore();
  store.update({
    markets: [MARKET],
    loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
  });
  const exchange = new RustEngineExchange(
    'hyperliquid',
    { key: 'api-wallet', secret: 'private-key', ...options },
    store,
  );
  return { exchange, construct, commands, result, load };
};

describe('Rust Hyperliquid wrapper', () => {
  const exchanges: RustEngineExchange[] = [];
  const setup = (options: Partial<ExchangeOptions> = {}) => {
    const f = fixture(options);
    exchanges.push(f.exchange);
    return f;
  };
  afterEach(() => {
    exchanges.splice(0).forEach((exchange) => exchange.dispose());
  });

  it('registers perps and forwards wallet, vault and builder settings together', async () => {
    expect(isRustVenue('hyperliquid')).toBe(true);
    expect(isRustVenue('hyperliquidspot')).toBe(false);
    const hyperliquidBuilder = { address: '0xbuilder', feeTenthsBps: 30 };
    const { exchange, construct } = setup({
      applicationId: 'master-wallet',
      vault: 'vault-wallet',
      hyperliquidBuilder,
      testnet: true,
    });
    await exchange.start();
    expect(construct).toHaveBeenCalledOnce();
    const [venue, json] = construct.mock.calls[0];
    expect(venue).toBe('hyperliquid');
    expect(JSON.parse(json)).toEqual({
      key: 'api-wallet',
      secret: 'private-key',
      applicationId: 'master-wallet',
      vault: 'vault-wallet',
      hyperliquidBuilder,
      testnet: true,
      publicDataOnly: false,
      enableSpotMarkets: false,
    });
  });

  it.each([
    { applicationId: 'master-wallet', key: '', secret: '' },
    { applicationId: 'master-wallet', key: '', secret: 'private-key' },
    { vault: 'vault-wallet', key: '', secret: '' },
    { key: 'legacy-wallet', secret: '' },
  ])('starts identity-based account reads without signing credentials: %j', async (options) => {
    const { exchange, construct } = setup(options);
    await exchange.start();
    expect(exchange.isPublicDataOnly).toBe(false);
    expect(JSON.parse(construct.mock.calls[0][1]).publicDataOnly).toBe(false);
  });

  it('honors explicit public-only mode even with a wallet identity', async () => {
    const { exchange, construct } = setup({ applicationId: 'master-wallet', publicDataOnly: true });
    await exchange.start();
    expect(exchange.isPublicDataOnly).toBe(true);
    expect(JSON.parse(construct.mock.calls[0][1]).publicDataOnly).toBe(true);
  });

  it('rejects spot opt-in before loading or starting an engine', async () => {
    const { exchange, load, construct } = setup({ enableSpotMarkets: true });
    await expect(exchange.start()).rejects.toThrow('supports perpetual markets only');
    expect(load).not.toHaveBeenCalled();
    expect(construct).not.toHaveBeenCalled();
  });

  it.each([
    { type: OrderType.StopLoss, triggerPrice: 95 },
    { stopLoss: 95, takeProfit: 110 },
  ])('preserves a caller cloid and trigger/bracket shape: %j', async (shape) => {
    const { exchange, commands, result } = setup();
    await exchange.start();
    result.mockReturnValue(['42']);
    const opts = placement({ ...shape, clientOrderId: CLOID });
    await expect(
      exchange.placeOrderWithClientOrderId(opts as PlaceOrderOpts & { clientOrderId: string }),
    ).resolves.toEqual({ orderIds: ['42'], clientOrderId: CLOID });
    expect(commands).toEqual([{ op: 'placeOrders', orders: [opts], command: 1 }]);
  });

  it('rejects a whole batch before issuing any command when a cloid is invalid or repeated', async () => {
    const { exchange, commands } = setup();
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    await exchange.start();
    await expect(exchange.placeOrders([placement(), placement({ clientOrderId: 'invalid' })])).rejects.toThrow(
      /clientOrderId must/,
    );
    await expect(
      exchange.placeOrders([placement({ clientOrderId: CLOID }), placement({ clientOrderId: CLOID })]),
    ).rejects.toThrow(/duplicated/);
    expect(commands).toEqual([]);
  });

  it('preserves the replacement cloid on the returned array, including no venue IDs', async () => {
    const { exchange, commands, result } = setup();
    await exchange.start();
    result.mockReturnValue({ orderIds: [], clientOrderId: CLOID });
    const ids = await exchange.updateOrder({ order: ORDER, update: { price: 105 } });
    expect(Array.isArray(ids)).toBe(true);
    expect(ids).toHaveLength(0);
    expect(replacementClientOrderIdOf(ids)).toBe(CLOID);
    expect(commands).toEqual([{ op: 'amendOrder', id: '42', price: 105, command: 1 }]);
  });

  it('forwards lookup symbol and cloid and preserves the engine verdict', async () => {
    const { exchange, commands, result } = setup();
    await exchange.start();
    result.mockReturnValue({ found: false, definitive: false });
    await expect(exchange.findOrderByClientOrderId(MARKET.symbol, CLOID)).resolves.toEqual({
      found: false,
      definitive: false,
    });
    result.mockReturnValue({ found: true, order: ORDER });
    await expect(exchange.findOrderByClientOrderId(MARKET.symbol, CLOID)).resolves.toEqual({
      found: true,
      order: ORDER,
    });
    expect(commands.map(({ command: _id, ...command }) => command)).toEqual([
      { op: 'findOrderByClientOrderId', symbol: MARKET.symbol, clientOrderId: CLOID },
      { op: 'findOrderByClientOrderId', symbol: MARKET.symbol, clientOrderId: CLOID },
    ]);
  });
});
