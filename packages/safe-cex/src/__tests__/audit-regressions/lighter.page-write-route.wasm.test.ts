import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { setProxyToken } from '../../accessToken';
import { createExchangeFromAccount } from '../../createExchangeFromAccount';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { setDefaultTransport } from '../../transport/registry';
import { tealstreetTransport } from '../../transport/tealstreet/tealstreetTransport';
import { OrderSide, OrderType } from '../../types';
import { prepareRecordedHttp } from '../support/managedHttp';

const NOW = 1700000000000;
const KEY = `01${'00'.repeat(39)}`;
const cleanup: Array<() => void> = [];
afterEach(() => cleanup.splice(0).forEach((dispose) => dispose()));

const recorded = (
  accountLimits: unknown = { code: 200, user_tier_name: 'premium' },
  integrator = 158385,
) => {
  const requests: Array<{ method: string; url: string; body?: string }> = [];
  const sockets: Array<{ subscriptions: string[]; message(text: string): void }> = [];
  let marginMode = 0;
  let marginFraction = 1000;
  const response = (data: unknown) => ({ status: 200, body: new TextEncoder().encode(JSON.stringify(data)) });
  const transport: Transport = {
    now: () => NOW,
    prepareHttp: prepareRecordedHttp,
    connect: (_url, handlers) => {
      const subscriptions: string[] = [];
      sockets.push({ subscriptions, message: handlers.onMessage });
      queueMicrotask(() => handlers.onOpen());
      return { send: (text) => subscriptions.push(text), close() {} };
    },
    request: async (request) => {
      requests.push(request);
      const { pathname } = new URL(request.url);
      if (pathname.endsWith('/orderBookDetails'))
        return response({
          code: 200,
          order_book_details: [
            {
              market_id: 1,
              symbol: 'ETH',
              status: 'active',
              market_type: 'perp',
              supported_size_decimals: 3,
              supported_price_decimals: 2,
              min_base_amount: '0.001',
              min_quote_amount: '1',
              min_initial_margin_fraction: 500,
              maintenance_margin_fraction: 250,
            },
          ],
        });
      if (pathname.endsWith('/exchangeStats'))
        return response({
          code: 200,
          order_book_stats: [
            {
              market_id: 1,
              last_trade_price: '100',
              mark_price: '100',
              index_price: '100',
            },
          ],
        });
      if (pathname.endsWith('/account'))
        return response({
          code: 200,
          accounts: [
            {
              index: 718895,
              collateral: '1000',
              total_asset_value: '1000',
              available_balance: '900',
              approved_integrators: [
                {
                  account_index: integrator,
                  approval_expiry: NOW + 86400000,
                  max_perps_maker_fee: 0,
                  max_perps_taker_fee: 250,
                },
              ],
              positions: [
                {
                  market_id: 1,
                  position: '2',
                  sign: 1,
                  avg_entry_price: '100',
                  unrealized_pnl: '0',
                  margin_mode: marginMode,
                  initial_margin_fraction: marginFraction,
                  liquidation_price: '50',
                },
              ],
            },
          ],
        });
      if (pathname.endsWith('/accountLimits')) return response(accountLimits);
      if (pathname.endsWith('/accountActiveOrders')) return response({ code: 200, orders: [] });
      if (pathname.endsWith('/nextNonce')) return response({ code: 200, nonce: 10 });
      if (pathname.endsWith('/sendTx')) {
        const params = new URLSearchParams(request.body);
        const tx = JSON.parse(params.get('tx_info')!);
        if (params.get('tx_type') === '20') {
          marginMode = tx.MarginMode;
          marginFraction = tx.InitialMarginFraction;
        }
        return response({ code: 200, tx_hash: 'documented-transaction-hash' });
      }
      throw new Error(`Unexpected Lighter boundary request ${request.method} ${request.url}`);
    },
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  };
  return { transport, requests, sockets };
};

const account = (flags: object) =>
  ({ exchange: 'lighter', name: 'row', apiKey: '', apiSecret: KEY, apiKeyIndex: '0', applicationId: '718895', ...flags }) as any;
// Legacy's rule (#2833): Lighter REST goes through the public CORS proxy whatever the boxes say.
const PUBLIC_PROXY = 'https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy/https://mainnet.zklighter.elliot.ai/api/v1/sendTx';
const place = { symbol: 'ETH', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.2, price: 90 };

/** The form hides the REST proxy box on Lighter (forbidCors); its one box sets tealstreetWsRoute. */
describe('where a browser-tab Lighter write goes', () => {
  it.each([
    ['the REST proxy flag, which the Lighter form does not offer', { tealstreetProxy: true }, PUBLIC_PROXY],
    ['the WebSocket Route box, the one the Lighter form offers', { tealstreetWsRoute: true }, PUBLIC_PROXY],
    ['no box', {}, PUBLIC_PROXY],
  ])('%s', async (_label, flags, expected) => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.reject(new Error('no network in this test'))));
    setDefaultTransport(tealstreetTransport);
    setProxyToken('test-token');
    const tape = recorded();
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    const sendTx = () => tape.requests.filter((r) => new URL(r.url).pathname.endsWith('/sendTx')).map((r) => r.url);
    const worker = (await createExchangeFromAccount(account(flags))) as unknown as RustEngineExchange;
    cleanup.push(() => worker.dispose());
    await worker.start();
    await worker.placeOrder(place);
    expect(sendTx()).toEqual([expected]);
    tape.requests.length = 0;
    const page = (await createExchangeFromAccount(account(flags), { workerMirror: true } as any)) as unknown as RustEngineExchange;
    cleanup.push(() => page.dispose());
    await expect(page.frameFedReady).resolves.toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 50));
    page.ingestSourceFrame(worker.snapshotFrame()!, true);
    await new Promise((resolve) => setTimeout(resolve, 50));
    await page.placeOrder(place);
    expect(sendTx()).toEqual([expected]);
  });
});
