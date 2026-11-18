import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, Order, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import qs from 'qs';
import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { prepareRecordedHttp } from '../support/managedHttp';

const STAMP = 1_700_000_000_000;
const KEY = 'bingx-api-key';
const MATERIAL = 'bingx-test-secret';
const LISTEN_KEY = '/openApi/user/auth/userDataStream';
const OPEN = { orderId: '777', symbol: 'BTC-USDT', side: 'BUY', positionSide: 'BOTH', type: 'LIMIT', origQty: '0.012',
  executedQty: '0', price: '89000', status: 'NEW', timeInForce: 'GTC', reduceOnly: false, time: STAMP, updateTime: STAMP };
const live: { dispose(): void }[] = [];
afterEach(() => {
  live.splice(0).forEach((item) => item.dispose());
  setWasmEngineLoader(loadNodeEngine); setRustEngineTransport();
});

type Sent = Parameters<Transport['request']>[0];
/** The documented BingX envelopes, recording every request with no live I/O. */
const tape = (sent: Sent[]): Transport => ({
  prepareHttp: prepareRecordedHttp,
  now: () => STAMP,
  connect: (_url, { onOpen }) => { setTimeout(onOpen, 0); return { send: () => {}, close: () => {} }; },
  request: async (request) => {
    sent.push(request);
    const path = new URL(request.url).pathname;
    const data = ({
      '/openApi/swap/v2/quote/contracts': [{ symbol: 'BTC-USDT', asset: 'BTC', currency: 'USDT', status: 1,
        quantityPrecision: 3, pricePrecision: 1, tradeMinLimit: '0.001', maxLongLeverage: 125 }],
      '/openApi/swap/v2/quote/ticker': [{ symbol: 'BTC-USDT', lastPrice: '90000', priceChangePercent: '1', volume: '2', quoteVolume: '180000' }],
      '/openApi/swap/v2/quote/premiumIndex': [{ symbol: 'BTC-USDT', markPrice: '90000', indexPrice: '89999', lastFundingRate: '0.01', nextFundingTime: 1760889600000 }],
      '/openApi/swap/v1/positionSide/dual': { dualSidePosition: 'false' },
      '/openApi/swap/v3/user/balance': [{ asset: 'USDT', balance: '1000', availableMargin: '900', usedMargin: '100', unrealizedProfit: '0' }],
      '/openApi/swap/v2/user/positions': [],
      '/openApi/swap/v2/trade/openOrders': { orders: [OPEN] },
      '/openApi/swap/v2/trade/order': { order: { ...OPEN, orderId: request.method === 'DELETE' ? '777' : '4242', status: request.method === 'DELETE' ? 'CANCELED' : 'NEW' } },
      '/openApi/swap/v1/trade/amend': { orderId: '777', clientOrderId: '', symbol: 'BTC-USDT', quantity: '0.02' },
    } as Record<string, unknown>)[path];
    if (path === LISTEN_KEY) return { status: 200, headers: '', body: new TextEncoder().encode('{"listenKey":"documented-test-key"}') };
    if (data === undefined) throw new Error(`Unexpected BingX request: ${request.method} ${path}`);
    return { status: 200, headers: '', body: new TextEncoder().encode(JSON.stringify({ code: 0, msg: '', data })) };
  },
  setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
});

/** Start one account, then place, amend and cancel through it; returns every request sent. */
const session = async (options: ExchangeOptions) => {
  const sent: Sent[] = [];
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => tape(sent));
  const store = new DefaultStore();
  const exchange = new RustEngineExchange('bingx', options, store);
  live.push(exchange);
  await exchange.start();
  await exchange.placeOrders([{ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.012, price: 90000, clientOrderId: 'caller_bingx-01' }]);
  const open = store.orders.find((order) => order.id === '777') as Order;
  await exchange.updateOrder({ order: open, update: { amount: 0.02 } });
  await exchange.cancelOrders([open]);
  return sent;
};

/** The keyed browser: an actual WASM engine answering through its hosted capture. */
const browser = () => {
  const exchange = new RustEngineExchange('bingx', { key: KEY, secret: MATERIAL }, new DefaultStore());
  live.push(exchange);
  return exchange;
};

const writes = (sent: Sent[]) => sent.filter(({ url, method }) => method !== 'GET' || new URL(url).pathname === LISTEN_KEY);
const text = (body: Sent['body']) => typeof body === 'string' ? body : new TextDecoder().decode(body as unknown as Uint8Array);
/** The legacy TS client's preimage: `localeCompare`-sorted repeat-format params minus the signature. */
const legacyPreimage = ({ url, body }: Sent) => {
  const query = new URL(url).search.slice(1);
  const { signature: _signature, ...params } = body ? JSON.parse(text(body)) : qs.parse(query);
  const sort = (a: string, b: string) => a.localeCompare(b);
  return qs.stringify(Object.fromEntries(Object.keys(params).sort(sort).map((key) => [key, params[key]])), { arrayFormat: 'repeat', sort });
};
const signatureOf = ({ url, body }: Sent) => body
  ? JSON.parse(text(body)).signature : new URL(url).searchParams.get('signature');

it('a keyless remote-signed BingX account sends the local-key bytes for place, amend, cancel and the listen key', async () => {
  const local = writes(await session({ key: KEY, secret: MATERIAL }));
  const keyed = browser();
  const sign = vi.fn((request: RemoteSignerRequest) => keyed.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'hosted-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const remote = writes(await session({ key: KEY, secret: '', remoteSigner, privateOnly: true }));

  const paths = local.map(({ method, url }) => `${method} ${new URL(url).pathname}`);
  for (const write of ['POST /openApi/swap/v2/trade/order', 'POST /openApi/swap/v1/trade/amend', 'DELETE /openApi/swap/v2/trade/order', `POST ${LISTEN_KEY}`])
    expect(paths).toContain(write);
  expect(remote.map(({ method, url, headers, body }) => ({ method, url, headers, body })))
    .toEqual(local.map(({ method, url, headers, body }) => ({ method, url, headers, body })));
  for (const request of local) {
    expect(request.headers).toMatchObject({ 'X-BX-APIKEY': KEY });
    if (new URL(request.url).pathname === LISTEN_KEY) { expect(signatureOf(request)).toBeFalsy(); continue; }
    // Independent Node crypto over the legacy TS preimage, not the engine's own serializer.
    expect(signatureOf(request)).toBe(createHmac('sha256', MATERIAL).update(legacyPreimage(request)).digest('hex'));
  }
  const signed = local.filter((request) => new URL(request.url).pathname !== LISTEN_KEY);
  // Signed reads (balance, positions, open orders) take the same wire; the listen key never does.
  expect(sign.mock.calls.every(([request]) => request.type === 'ws-auth' && request.wsAuth?.outputFormat === 'hex'
    && !request.wsAuth.toSign.includes('listenKey'))).toBe(true);
  for (const request of signed)
    expect(sign).toHaveBeenCalledWith(expect.objectContaining({ type: 'ws-auth', exchangeName: 'bingx', accountName: 'hosted-account',
      wsAuth: { toSign: legacyPreimage(request), outputFormat: 'hex' } }));
});

it('the keyed BingX browser answers hex ws-auth with the Node HMAC and refuses the Bitunix scheme', async () => {
  const keyed = browser();
  const toSign = `price=90000&quantity=0.012&side=BUY&symbol=BTC-USDT&timestamp=${STAMP}`;
  await expect(keyed.signWsAuth({ id: 'hex', type: 'ws-auth', exchangeName: 'bingx', wsAuth: { toSign, outputFormat: 'hex' } }))
    .resolves.toEqual({ id: 'hex', wsAuthSignature: createHmac('sha256', MATERIAL).update(toSign).digest('hex') });
  await expect(keyed.signWsAuth({ id: 'fused', type: 'ws-auth', exchangeName: 'bingx', wsAuth: { toSign, outputFormat: 'bitunix-double-sha256' } }))
    .resolves.toMatchObject({ error: expect.any(String) });
  expect(keyed.hasStarted).toBe(false);
});

it('a keyless BingX payloadSigner account is refused before any request', () => {
  setWasmEngineLoader(loadNodeEngine);
  expect(() => new RustEngineExchange('bingx', { key: KEY, secret: '', payloadSigner: vi.fn() }, new DefaultStore()))
    .toThrow('no REST-only PayloadSigner lifecycle');
});
