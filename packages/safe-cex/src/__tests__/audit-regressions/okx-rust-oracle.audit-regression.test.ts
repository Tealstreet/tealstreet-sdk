import type { Market } from '../../types';

import { expect, it, vi } from 'vitest';

import { OKXExchange } from '../../exchanges/okx/okx.exchange';
import { DefaultStore } from '../../store/store.base';

const setup = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC-USDT-SWAP',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        contractSize: 0.01,
        precision: { amount: 0.01, price: 0.1 },
      } as Market,
    ],
  });
  return new OKXExchange('okx', { key: '', secret: '', passphrase: 'fixture', publicDataOnly: true }, store);
};

it('reads public trade contract quantities as base with millisecond time', async () => {
  const exchange = setup();
  vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
    data: {
      data: [{ instId: 'BTC-USDT-SWAP', tradeId: '123', px: '40000', sz: '5', ts: '1710000000000', side: 'buy' }],
    },
  });
  await expect(exchange.fetchPublicTrades('BTCUSDT')).resolves.toEqual([
    { id: '123', symbol: 'BTCUSDT', price: 40000, qty: 0.05, quoteQty: 2000, timestamp: 1710000000000, side: 'buy' },
  ]);
  exchange.dispose();
});

it('uses UTC month bounds even when local dates cross into the next day', async () => {
  const exchange = setup();
  const get = vi
    .spyOn(exchange.xhr, 'get')
    .mockResolvedValue({ data: { data: [['1709236800000', '1', '2', '0.5', '1', '1', '1', '1']] } });
  await exchange.fetchOHLCV({
    symbol: 'BTCUSDT',
    interval: '1M' as never,
    limit: 1,
    to: Date.parse('2024-03-30T20:00:00Z'),
  });
  const params = get.mock.calls[0][1]?.params as { before: number };
  expect(params.before).toBe(Date.parse('2024-02-29T20:00:00Z'));
  exchange.dispose();
});

it('subscribes keyless business candles without login or private topics', async () => {
  const { OKXBusinessWebsocket } = await import('../../exchanges/okx/okx.ws-business');
  const exchange = setup();
  const socket = new OKXBusinessWebsocket(exchange);
  socket.topics.candle = [{ channel: 'candle1m', instId: 'BTC-USDT-SWAP' }];
  const send = vi.spyOn(socket, 'sendWithLog').mockImplementation(() => {});
  const auth = vi.spyOn(socket, 'auth');
  await socket.onOpen();
  expect(auth).not.toHaveBeenCalled();
  expect(send.mock.calls.map(([text]) => text)).toEqual([
    JSON.stringify({ op: 'subscribe', args: [{ channel: 'candle1m', instId: 'BTC-USDT-SWAP' }] }),
    'ping',
  ]);
  exchange.dispose();
});
