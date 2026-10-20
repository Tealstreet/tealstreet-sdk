import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions, Market, PlaceOrderOpts } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { isRustVenue, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const credentials = { key: `0x${'1'.padStart(64, '0')}`, paradexAddress: '0x123', subkeyAddress: '0x456' };
const market: Market = {
  id: 'BTC-USD-PERP', symbol: 'BTC-USD-PERP', base: 'BTC', quote: 'USD', active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 50 } }, info: {},
};
const placement = (extra: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: market.symbol, type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 100, ...extra,
});

describe('Rust Paradex account boundary', () => {
  const exchanges: RustEngineExchange[] = [];
  const setup = (options: Partial<ExchangeOptions> = {}) => {
    const commands: Array<Record<string, unknown>> = [];
    const noActions = () => '[]';
    const engine: WasmEngineLike = {
      start: noActions,
      command: (json) => {
        const command = JSON.parse(json);
        if (command.op === 'setTickerSymbols') return noActions();
        commands.push(command);
        return JSON.stringify([{ op: 'done', command: command.command, result: '["42"]' }]);
      },
      takeFrame: () => undefined, snapshotFrame: () => new Uint8Array(),
      wsOpen: noActions, wsMessage: noActions, wsMessages: noActions, wsClose: noActions,
      httpStart: noActions, httpChunk: noActions, httpEnd: noActions, httpFailed: noActions, timer: noActions,
    };
    const construct = vi.fn(function (_venue: string, _options: string) { return engine; });
    const load = vi.fn(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
    setWasmEngineLoader(load);
    const store = new DefaultStore();
    store.update({ markets: [market], loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
    const exchange = new RustEngineExchange('paradex', { ...credentials, secret: '', ...options }, store);
    vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
    exchanges.push(exchange);
    return { exchange, load, construct, commands };
  };
  afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

  it('registers Paradex and forwards its secret-free account and testnet', async () => {
    expect(isRustVenue('paradex')).toBe(true);
    const { exchange, construct } = setup({ testnet: true });
    expect(exchange.isPublicDataOnly).toBe(false);
    await exchange.start();
    expect(construct).toHaveBeenCalledOnce();
    expect(construct.mock.calls[0][0]).toBe('paradex');
    expect(JSON.parse(construct.mock.calls[0][1])).toMatchObject({ ...credentials, publicDataOnly: false, testnet: true });
  });

  it.each([{ key: '' }, { paradexAddress: '' }, { subkeyAddress: '' }, { publicDataOnly: true }])(
    'uses public data when a required credential is missing or explicitly public: %j', async (options) => {
      const { exchange, construct } = setup(options);
      expect(exchange.isPublicDataOnly).toBe(true);
      await exchange.start();
      expect(JSON.parse(construct.mock.calls[0][1]).publicDataOnly).toBe(true);
    },
  );

  it.each([{ key: '' }, { paradexAddress: '' }, { subkeyAddress: '' }])(
    'rejects explicitly requested private mode with missing fields before loading: %j', async (missing) => {
      const { exchange, load, construct } = setup({ ...missing, publicDataOnly: false });
      await expect(exchange.start()).rejects.toThrow('private accounts require a local subkey');
      expect(load).not.toHaveBeenCalled();
      expect(construct).not.toHaveBeenCalled();
    },
  );

  it('rejects a remote payload signer before loading or starting an engine', async () => {
    const payloadSigner = vi.fn(async () => ({ signature: '["1","2"]' }));
    const { exchange, load, construct } = setup({ payloadSigner });
    await expect(exchange.start()).rejects.toThrow('remote signing is unsupported');
    expect(load).not.toHaveBeenCalled();
    expect(construct).not.toHaveBeenCalled();
    expect(payloadSigner).not.toHaveBeenCalled();
  });

  it('honors caller IDs verbatim and mints bounded attributed IDs', async () => {
    const { exchange, commands } = setup();
    await exchange.start();
    const opts = placement({ clientOrderId: 'Caller_Paradex-01' });
    await expect(exchange.placeOrderWithClientOrderId(opts as PlaceOrderOpts & { clientOrderId: string })).resolves.toEqual({
      orderIds: ['42'], clientOrderId: opts.clientOrderId,
    });
    expect(commands).toEqual([{ op: 'placeOrders', orders: [opts], command: 1 }]);
    expect(exchange.mintClientOrderId(placement())).toMatch(/^ts_.{1,61}$/);
  });

  it('rejects invalid IDs, close, brackets, splits and duplicate batches before a command', async () => {
    const { exchange, commands } = setup();
    await exchange.start();
    for (const opts of [
      placement({ clientOrderId: '' }), placement({ clientOrderId: 'x'.repeat(65) }),
      placement({ clientOrderId: 'close', close: true }), placement({ clientOrderId: 'bracket', stopLoss: 90 }),
      placement({ clientOrderId: 'split', amount: 11 }),
    ]) await expect(exchange.placeOrder(opts)).rejects.toThrow(/clientOrderId/);
    await expect(exchange.placeOrders([placement(), placement({ clientOrderId: 'x'.repeat(65) })])).rejects.toThrow(/clientOrderId/);
    await expect(exchange.placeOrders([placement({ clientOrderId: 'same' }), placement({ clientOrderId: 'same' })])).rejects.toThrow(/duplicated/);
    expect(commands).toEqual([]);
  });
});
