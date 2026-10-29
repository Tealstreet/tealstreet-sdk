import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { prepareRecordedHttp } from '../support/managedHttp';

const NOW = 1_700_000_000_000;
// Fixture-only 40-byte scalar from the Lighter boundary test.
const SCALAR = `01${'00'.repeat(39)}`;
const ACCOUNT = { apiKeyIndex: '0', applicationId: '718895', testnet: true } as const;
const live: { dispose(): void }[] = [];
afterEach(() => {
  live.splice(0).forEach((item) => item.dispose());
  setWasmEngineLoader(loadNodeEngine); setRustEngineTransport();
});

type Sent = Parameters<Transport['request']>[0];
const json = (data: unknown) => ({ status: 200, headers: '', body: new TextEncoder().encode(JSON.stringify(data)) });
/** The documented Lighter envelopes, recording every request with no live I/O. */
const tape = (sent: Sent[]): Transport => ({
  now: () => NOW,
  prepareHttp: prepareRecordedHttp,
  connect: (_url, handlers) => { queueMicrotask(() => handlers.onOpen()); return { send: () => {}, close() {} }; },
  request: async (request) => {
    sent.push(request);
    const { pathname } = new URL(request.url);
    if (pathname.endsWith('/orderBookDetails')) return json({ code: 200, order_book_details: [{ market_id: 1, symbol: 'ETH',
      status: 'active', market_type: 'perp', supported_size_decimals: 3, supported_price_decimals: 2, min_base_amount: '0.001',
      min_quote_amount: '1', min_initial_margin_fraction: 500, maintenance_margin_fraction: 250 }] });
    if (pathname.endsWith('/exchangeStats'))
      return json({ code: 200, order_book_stats: [{ market_id: 1, last_trade_price: '100', mark_price: '100', index_price: '100' }] });
    if (pathname.endsWith('/account')) return json({ code: 200, accounts: [{ index: 718895, collateral: '1000', total_asset_value: '1000',
      available_balance: '900', approved_integrators: [{ account_index: 158385, approval_expiry: NOW + 86400000,
        max_perps_maker_fee: 0, max_perps_taker_fee: 250 }], positions: [] }] });
    if (pathname.endsWith('/accountLimits')) return json({ code: 200, user_tier_name: 'premium' });
    if (pathname.endsWith('/accountActiveOrders')) return json({ code: 200, orders: [] });
    if (pathname.endsWith('/nextNonce')) return json({ code: 200, nonce: 10 });
    if (pathname.endsWith('/sendTx')) return json({ code: 200, tx_hash: 'documented-transaction-hash' });
    throw new Error(`Unexpected Lighter request ${request.method} ${request.url}`);
  },
  setTimeout, clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
});

/** Start one account and place one limit order; returns its sendTx form. */
const place = async (options: ExchangeOptions) => {
  const sent: Sent[] = [];
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => tape(sent));
  const exchange = new RustEngineExchange('lighter', options, new DefaultStore());
  live.push(exchange);
  await exchange.start();
  await exchange.placeOrders([{ symbol: 'ETH', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.5, price: 100 }]);
  const tx = sent.find(({ url }) => new URL(url).pathname.endsWith('/sendTx'));
  const form = new URLSearchParams(typeof tx?.body === 'string' ? tx.body : new TextDecoder().decode(tx?.body as unknown as Uint8Array));
  return { type: form.get('tx_type'), info: JSON.parse(form.get('tx_info')!) };
};

it('a keyless Lighter account signs through the browser and sends the local-key transaction', async () => {
  const local = await place({ key: '', secret: SCALAR, ...ACCOUNT });
  // The keyed browser: an actual WASM engine answering through its hosted capture.
  const keyed = new RustEngineExchange('lighter', { key: '', secret: SCALAR, ...ACCOUNT }, new DefaultStore());
  live.push(keyed);
  const sign = vi.fn((request: RemoteSignerRequest) => keyed.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'hosted-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const remote = await place({ key: 'lighter-public-key', secret: '', remoteSigner, privateOnly: true, ...ACCOUNT });

  expect(remote.type).toBe('14');
  // Hedged signatures differ by entropy and each engine mints its own client
  // order index; every other signed field is identical.
  const { Sig: localSig, ClientOrderIndex: localIndex, ...localFields } = local.info;
  const { Sig: remoteSig, ClientOrderIndex: remoteIndex, ...remoteFields } = remote.info;
  expect(remoteFields).toEqual(localFields);
  expect([typeof localIndex, typeof remoteIndex]).toEqual(['number', 'number']);
  expect(Buffer.from(remoteSig, 'base64')).toHaveLength(80);
  expect(localSig).toBeTruthy();
  const calls = sign.mock.calls.map(([request]) => request);
  expect(calls.every((request) => request.type === 'ws-auth' && request.exchangeName === 'lighter'
    && request.wsAuth?.outputFormat === 'lighter-tx')).toBe(true);
  const statement = calls.map((request) => JSON.parse(request.wsAuth!.toSign)).find((item) => item.kind === 'txs');
  expect(statement).toMatchObject({ chainId: 300, accountIndex: 718895, apiKeyIndex: 0,
    txs: [{ txType: 14, nonce: localFields.Nonce, expiredAt: localFields.ExpiredAt,
      order: { ClientOrderIndex: remoteIndex, BaseAmount: localFields.BaseAmount, Price: localFields.Price } }] });
  expect(keyed.hasStarted).toBe(false);
});

it('the keyed Lighter browser refuses a statement for another account', async () => {
  const keyed = new RustEngineExchange('lighter', { key: '', secret: SCALAR, ...ACCOUNT }, new DefaultStore());
  live.push(keyed);
  const toSign = '{"kind":"auth","chainId":300,"accountIndex":1,"apiKeyIndex":0,"deadline":1700000060}';
  await expect(keyed.signWsAuth({ id: 'other', type: 'ws-auth', exchangeName: 'lighter', wsAuth: { toSign, outputFormat: 'lighter-tx' } }))
    .resolves.toMatchObject({ error: expect.any(String) });
  const own = toSign.replace('"accountIndex":1', '"accountIndex":718895');
  await expect(keyed.signWsAuth({ id: 'own', type: 'ws-auth', exchangeName: 'lighter', wsAuth: { toSign: own, outputFormat: 'lighter-tx' } }))
    .resolves.toMatchObject({ wsAuthSignature: expect.stringMatching(/^0x[0-9a-f]{160}$/) });
});
