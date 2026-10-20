import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { PayloadSignerRequest } from '../../types';

import { expect, it } from 'vitest';

import vectors from '../../../../../sdk-rs/venues/paradex/tests/sign-vectors.json';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

it('delegates exact StarkNet hashes through the SDK and wasm host before sending JWT or order requests', async () => {
  const vector = vectors.cases[0];
  const requests: Array<Parameters<Transport['request']>[0]> = [];
  const signatures: PayloadSignerRequest[] = [];
  const errors: unknown[] = [];
  const response = (body: unknown) => ({ status: 200, body: new TextEncoder().encode(JSON.stringify(body)) });
  const transport: Transport = {
    now: () => vector.now,
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
    connect: (_url, handlers) => {
      queueMicrotask(() => handlers.onOpen());
      return {
        close() {},
        send(text) {
          const message = JSON.parse(text);
          if (message.method === 'auth') {
            queueMicrotask(() => handlers.onMessage(JSON.stringify({ jsonrpc: '2.0', id: message.id, result: {} })));
          }
        },
      };
    },
    request: async (request) => {
      requests.push(request);
      const path = new URL(request.url).pathname;
      if (path.endsWith('/system/config')) return response({ starknet_chain_id: vector.chain });
      if (path.endsWith('/markets/summary'))
        return response({ results: [{ symbol: 'BTC-USD-PERP', last_traded_price: '1' }] });
      if (path.endsWith('/markets'))
        return response({
          results: [
            {
              symbol: 'BTC-USD-PERP',
              base_currency: 'BTC',
              quote_currency: 'USD',
              asset_kind: 'PERP',
              order_size_increment: '0.00000001',
              price_tick_size: '0.00000001',
              min_notional: '0',
              max_order_size: '100',
            },
          ],
        });
      if (path.includes('/auth/')) return response({ jwt_token: 'delegated-jwt' });
      if (path.endsWith('/account/margin')) return response({ configs: [] });
      if (path.endsWith('/positions')) return response({ results: [] });
      if (path.endsWith('/account')) return response({ total_collateral: '1000', free_collateral: '1000' });
      if (path.endsWith('/orders/batch') && request.method === 'POST') {
        const [order] = JSON.parse(request.body!);
        return response({ orders: [{ id: 'placed-order', client_id: order.client_id }] });
      }
      if (path.endsWith('/orders')) return response({ results: [] });
      throw new Error(`Unexpected request ${request.method} ${request.url}`);
    },
  };
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => transport);
  const exchange = new RustEngineExchange(
    'paradex',
    {
      key: 'public-subkey-only',
      secret: '',
      paradexAddress: vector.address,
      subkeyAddress: vector.subkey,
      tealstreetProxy: false,
      tealstreetTunnel: false,
      payloadSigner(request) {
        signatures.push(request);
        return {
          signature: `opaque signer answer ${signatures.length}`,
          headers: {
            'X-Signer-Proof': `proof-${signatures.length}`,
            'PARADEX-STARKNET-ACCOUNT': 'cannot-overwrite-account',
          },
        };
      },
    },
    new DefaultStore(),
  );
  exchange.on('error', (error: unknown) => errors.push(error));
  const bounded = <T>(operation: Promise<T>) =>
    new Promise<T>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`Startup timed out: ${JSON.stringify({ errors, urls: requests.map((r) => r.url) })}`)),
        3000,
      );
      operation.then(resolve, reject).finally(() => clearTimeout(timer));
    });
  try {
    await bounded(exchange.start());
    expect(signatures[0]).toEqual({
      exchangeName: 'paradex',
      key: 'public-subkey-only',
      scheme: 'stark',
      encoding: 'felt-pair',
      payload: vector.auth.hash,
    });
    const auth = requests.find((request) => new URL(request.url).pathname.includes('/auth/'))!;
    expect(auth.url).toBe(`https://api.prod.paradex.trade/v1/auth/${vector.subkey}`);
    expect(auth.headers['PARADEX-STARKNET-SIGNATURE']).toBe('opaque signer answer 1');
    expect(auth.headers['PARADEX-STARKNET-ACCOUNT']).toBe(vector.address);
    expect(auth.headers['X-Signer-Proof']).toBe('proof-1');
    await expect(
      exchange.placeOrder({
        symbol: 'BTC-USD-PERP',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.00000999,
        price: 0.00000999,
      }),
    ).resolves.toEqual(['placed-order']);
    expect(signatures[1].payload).toBe(vector.orders[0].hash);
    const placed = requests.find((request) => request.url.endsWith('/orders/batch'))!;
    expect(JSON.parse(placed.body!)[0].signature).toBe('opaque signer answer 2');
    expect(placed.headers.Authorization).toBe('Bearer delegated-jwt');
    expect(placed.headers['X-Signer-Proof']).toBe('proof-2');
    expect(errors).toEqual([]);
  } finally {
    exchange.dispose();
  }
}, 10000);
