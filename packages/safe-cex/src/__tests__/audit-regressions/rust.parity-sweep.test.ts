import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { mapBinanceHistoricalOrder } from '../../exchanges/binance/binance.order-history';
import { OKXExchange } from '../../exchanges/okx/okx.exchange';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const exchanges: RustEngineExchange[] = [];
const fixture = (name: ExchangeName, options: Partial<ExchangeOptions> = {}) => {
  const commands: Array<Record<string, unknown>> = [];
  const verdict = vi.fn((): unknown => ({ found: false, definitive: true }));
  const empty = () => '[]';
  const engine: WasmEngineLike = {
    start: empty,
    command: (json) => {
      const command = JSON.parse(json);
      if (command.op === 'setTickerSymbols') return '[]';
      commands.push(command);
      return JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(verdict()) }]);
    },
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(),
    wsOpen: empty,
    wsMessage: empty,
    wsClose: empty,
    httpStart: empty,
    httpChunk: empty,
    httpEnd: empty,
    httpFailed: empty,
    timer: empty,
  };
  const construct = vi.fn(function (_venue: string, _options: string) {
    return engine;
  });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  const exchange = new RustEngineExchange(
    name,
    { key: 'primary-key', secret: 'primary-secret', passphrase: 'primary-pass', ...options },
    store,
  );
  exchanges.push(exchange);
  return { exchange, commands, verdict, construct };
};

afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

describe('Rust parity query forwarding', () => {
  it.each<ExchangeName>(['binance', 'binancepm', 'okx', 'okxeu', 'okxdemo', 'okxeudemo', 'phemex'])(
    '%s preserves the native definitive lookup verdict',
    async (name) => {
      const { exchange, commands, verdict } = fixture(name);
      await exchange.start();
      await expect(exchange.findOrderByClientOrderId('BTCUSDT', 'caller-id')).resolves.toEqual({
        found: false,
        definitive: true,
      });
      verdict.mockReturnValue({ found: false, definitive: false });
      await expect(exchange.findOrderByClientOrderId('UNKNOWN', 'caller-id')).resolves.toEqual({
        found: false,
        definitive: false,
      });
      expect(commands.map(({ command: _command, ...query }) => query)).toEqual([
        { op: 'findOrderByClientOrderId', symbol: 'BTCUSDT', clientOrderId: 'caller-id' },
        { op: 'findOrderByClientOrderId', symbol: 'UNKNOWN', clientOrderId: 'caller-id' },
      ]);
    },
  );
  it('forwards Phemex ledger bounds and kind without converting milliseconds or adding symbol scope', async () => {
    const { exchange, commands, verdict } = fixture('phemex');
    verdict.mockReturnValue([]);
    await exchange.start();
    await expect(
      exchange.fetchIncomeHistoryRange(1_700_000_000_000, 1_700_086_400_000, 'REALIZED_PNL'),
    ).resolves.toEqual([]);
    expect(commands).toEqual([
      expect.objectContaining({
        op: 'fetchIncome',
        from: 1_700_000_000_000,
        to: 1_700_086_400_000,
        incomeType: 'REALIZED_PNL',
      }),
    ]);
    expect(commands[0]).not.toHaveProperty('symbol');
  });
  it('forwards secondary credentials separately from the primary account', async () => {
    const { exchange, construct } = fixture('phemex', { secondary_key: 'read-key', secondary_secret: 'read-secret' });
    await exchange.start();
    expect(construct).toHaveBeenCalledOnce();
    expect(JSON.parse(construct.mock.calls[0]![1] as string)).toMatchObject({
      key: 'primary-key',
      secret: 'primary-secret',
      readCredentials: { key: 'read-key', secret: 'read-secret', passphrase: 'primary-pass' },
    });
  });
});

describe('OKX primary account lookup', () => {
  it('cannot turn a secondary account miss into definitive primary absence', async () => {
    const store = new DefaultStore();
    store.update({
      markets: [
        {
          id: 'BTC-USDT-SWAP',
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { price: 0.1, amount: 1 },
          limits: { amount: { min: 1, max: 100 }, leverage: { min: 1, max: 100 } },
          info: {},
        },
      ],
    });
    const exchange = new OKXExchange('okx', { key: 'primary', secret: 'secret', passphrase: 'pass' }, store);
    const primary = vi.fn().mockRejectedValue({ response: { data: { code: '51603' } } });
    const secondary = vi.fn().mockResolvedValue({ data: { data: [] } });
    exchange._primaryXhr = { get: primary } as unknown as typeof exchange.xhr;
    exchange.xhr = { get: secondary } as unknown as typeof exchange.xhr;
    try {
      await expect(exchange.findOrderByClientOrderId('BTCUSDT', 'caller')).resolves.toEqual({
        found: false,
        definitive: true,
      });
      expect(primary).toHaveBeenCalledWith('/api/v5/trade/order', {
        params: { instId: 'BTC-USDT-SWAP', clOrdId: 'caller' },
      });
      expect(secondary).not.toHaveBeenCalled();
    } finally {
      exchange.dispose();
    }
  });
});

it('maps Binance STP expiration to canceled instead of a filled-looking closed order', () => {
  expect(
    mapBinanceHistoricalOrder({
      orderId: '1',
      symbol: 'BTCUSDT',
      time: 1700000000000,
      status: 'EXPIRED_IN_MATCH',
      side: 'BUY',
      type: 'LIMIT',
      origQty: '1',
      executedQty: '0',
    })?.status,
  ).toBe('canceled');
});

it('keeps a live untraded OKX X-Perp ticker finite without dropping its market', () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'TEST002-USD_UM_XPERP-310822',
        symbol: 'TEST002USD_UM_XPERP310822',
        base: 'TEST002',
        quote: 'USD',
        active: true,
        precision: { price: 0.1, amount: 1 },
        limits: { amount: { min: 1, max: 100 }, leverage: { min: 1, max: 20 } },
        info: {},
      },
    ],
  });
  const exchange = new OKXExchange('okxeu', { key: '', secret: '', passphrase: 'pass' }, store);
  try {
    const tickers = exchange.parseTickers([
      {
        instId: 'TEST002-USD_UM_XPERP-310822',
        last: '',
        open24h: '',
        bidPx: '',
        askPx: '',
        volCcy24h: '0',
        vol24h: '0',
      },
    ]);
    expect(tickers).toHaveLength(1);
    expect(tickers[0]).toMatchObject({ last: 0, mark: 0, index: 0, percentage: 0, bid: 0, ask: 0 });
    store.update({ tickers });
    exchange.publicWebsocket.handleTickerEvents({
      data: [
        {
          instId: 'TEST002-USD_UM_XPERP-310822',
          last: '',
          open24h: '',
          bidPx: '',
          askPx: '',
          volCcy24h: '0',
          vol24h: '0',
        },
      ],
    });
    expect(store.tickers[0]).toMatchObject({ last: 0, percentage: 0, bid: 0, ask: 0 });
  } finally {
    exchange.dispose();
  }
});
