// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { gunzipSync } from 'node:zlib';
import { afterEach, beforeAll, expect, it } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { prepareRecordedHttp } from '../support/managedHttp';

// COSIGNED end to end with the real Bybit wasm: the page engine the web builds
// (a worker mirror that turns frame-fed) takes the server's SCXF frames, holds
// the account's orders and positions, and sends a manual write itself.
type Row = { type: string; method?: string; url?: string; status?: number; headers?: string; body?: string };
const recorded = new Map<string, Row>();
for (const line of gunzipSync(
  readFileSync(resolve(__dirname, '../../../../../sdk-rs/harness/fixtures/bybit-public.jsonl.gz')),
)
  .toString()
  .trim()
  .split('\n')) {
  const row = JSON.parse(line) as Row;
  if (row.type === 'http' && row.url && !recorded.has(row.url)) recorded.set(row.url, row);
}

const live: RustEngineExchange[] = [];
beforeAll(() => setWasmEngineLoader(loadNodeEngine));
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setRustEngineTransport();
});

const pageEngine = () => {
  const sent: { method: string; url: string }[] = [];
  const sockets: string[] = [];
  const transport: Transport = {
    now: () => Date.now(),
    prepareHttp: prepareRecordedHttp,
    request: async (request) => {
      sent.push({ method: request.method, url: request.url });
      const row = recorded.get(request.url);
      const body =
        row?.body ??
        (new URL(request.url).pathname === '/v5/order/create-batch'
          ? JSON.stringify({
              retCode: 0,
              retMsg: 'OK',
              result: { list: [{ category: 'linear', symbol: 'BTCUSDT', orderId: 'page-1', orderLinkId: '', createAt: `${Date.now()}` }] },
              retExtInfo: { list: [{ code: 0, msg: 'OK' }] },
              time: Date.now(),
            })
          : ['/v5/position/set-leverage', '/v5/position/switch-mode'].includes(new URL(request.url).pathname)
            ? JSON.stringify({ retCode: 0, retMsg: 'OK', result: {}, time: Date.now() })
            : JSON.stringify({ retCode: 10001, retMsg: `unrecorded ${request.url}` }));
      return { status: row?.status ?? 200, headers: '', body: new TextEncoder().encode(body) };
    },
    connect: (url) => {
      sockets.push(url);
      throw new Error('a page engine opens no venue socket');
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
  };
  setRustEngineTransport(() => transport);
  const options = { key: 'page-key', secret: 'page-mac-key', workerMirror: true } as ExchangeOptions;
  const exchange = new RustEngineExchange('bybit', options, new DefaultStore());
  live.push(exchange);
  // The hosted bridge: what the controller wires to the exchange-server facade.
  const calls: string[] = [];
  exchange.on('callMethod', (event: { requestId: string; method: string }) => {
    calls.push(event.method);
    exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result: null });
  });
  return { exchange, sent, sockets, calls };
};

// The server: a real keyed Bybit engine started from the recorded private tape.
// Its snapshot frame is what the exchange server sends; the hosted client hands
// those exact bytes to the page engine's ingestSourceFrame.
const BTC = 'BTCUSDT';
const privateTape = () => {
  const dir = mkdtempSync(join(tmpdir(), 'bybit-tape-'));
  const out = join(dir, 'bybit-private.jsonl');
  execFileSync(process.execPath, [resolve(__dirname, '../../../../../sdk-rs/harness/private-tape-bybit.mjs'), '--out', out]);
  const rows = readFileSync(out, 'utf8').trim().split('\n').map((line) => JSON.parse(line) as Row);
  rmSync(dir, { recursive: true });
  return new Map(rows.filter((row) => row.type === 'http' && row.method === 'GET').map((row) => [row.url!, row]));
};

const serverEngine = async () => {
  const tape = privateTape();
  const transport: Transport = {
    now: () => Date.now(),
    prepareHttp: prepareRecordedHttp,
    request: async (request) => {
      const row = tape.get(request.url) ?? recorded.get(request.url);
      if (!row) throw new Error(`server read not on the tape: ${request.method} ${request.url}`);
      return { status: row.status ?? 200, headers: row.headers ?? '', body: new TextEncoder().encode(row.body ?? '') };
    },
    // Its private stream is not needed for the snapshot the REST reconcile builds.
    connect: () => ({ send() {}, close() {} }),
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
  };
  setRustEngineTransport(() => transport);
  const server = new RustEngineExchange('bybit', { key: 'server-key', secret: 'server-mac-key' } as ExchangeOptions, new DefaultStore());
  live.push(server);
  await server.start();
  setRustEngineTransport();
  return server;
};

const settled = () => new Promise((resolve) => setTimeout(resolve, 600));

it('the COSIGNED page engine holds server state, writes in one request, and survives a snapshot reset', async () => {
  const server = await serverEngine();
  const serverOrders = server.store.orders.map((o) => o.id).sort();
  const serverPositions = server.store.positions.map((p) => p.symbol).sort();
  expect(serverOrders.length).toBeGreaterThan(0);
  expect(serverPositions.length).toBeGreaterThan(0);
  const snapshot = server.snapshotFrame()!;
  const { exchange, sent, sockets, calls } = pageEngine();
  await expect(exchange.frameFedReady).resolves.toBe(true);
  expect(exchange.isFrameFed).toBe(true);
  await settled();
  const startup = sent.splice(0);
  expect(startup.every((r) => r.method === 'GET' && r.url.includes('/v5/market/'))).toBe(true);

  exchange.ingestSourceFrame(snapshot, true);
  await settled();
  expect(exchange.store.orders.map((o) => o.id).sort()).toEqual(serverOrders);
  expect(exchange.store.positions.map((p) => p.symbol).sort()).toEqual(serverPositions);

  // A manual write: one venue request from the page engine, nothing over the hosted socket.
  await expect(
    exchange.placeOrder({ symbol: BTC, type: OrderType.Limit, side: OrderSide.Buy, amount: 0.01, price: 95_000 }),
  ).resolves.toEqual(['page-1']);
  expect(sent.map((r) => `${r.method} ${new URL(r.url).pathname}`)).toEqual(['POST /v5/order/create-batch']);
  expect(calls).toEqual([]);

  // A settings write goes out from the page engine too; Bybit holds its position
  // mode (settings.held) and the page asks the server to re-read it.
  sent.splice(0);
  await exchange.callMethodInWorker('setLeverage', BTC, 5);
  expect(sent.map((r) => `${r.method} ${new URL(r.url).pathname}`)).toEqual(['POST /v5/position/set-leverage']);
  sent.splice(0);
  await exchange.changePositionMode(true, BTC);
  expect(sent.map((r) => `${r.method} ${new URL(r.url).pathname}`)).toEqual(['POST /v5/position/switch-mode']);
  await settled();
  expect(calls).toContain('refreshAccountSettings');
  expect(calls).not.toContain('setLeverage');
  expect(calls).not.toContain('changePositionMode');

  // A server reconnect re-sends a snapshot: the same page engine re-feeds without blanking.
  const blanked: number[] = [];
  const unsubscribe = exchange.store.subscribe((data) => blanked.push(data.orders.length));
  exchange.ingestSourceFrame(server.snapshotFrame()!, true);
  await settled();
  unsubscribe?.();
  expect(blanked).not.toContain(0);
  expect(exchange.store.orders.map((o) => o.id).sort()).toEqual(serverOrders);
  expect(exchange.store.positions.map((p) => p.symbol).sort()).toEqual(serverPositions);
  expect(sockets).toEqual([]);
});
