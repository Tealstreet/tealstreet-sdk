// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { readFileSync } from 'node:fs';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { prepareRecordedHttp } from '../support/managedHttp';

type TapeRow = { type: string; method?: string; url?: string; body?: string };

const TAPE: TapeRow[] = readFileSync(
  new URL('../../../../../sdk-rs/harness/tapes/bybit-private.jsonl', import.meta.url),
  'utf8',
)
  .split('\n')
  .filter(Boolean)
  .map((line) => JSON.parse(line) as TapeRow);

const tapeBody = (url: string) =>
  TAPE.find((row) => row.type === 'http' && row.url === url)?.body;

const LONG_ROW = JSON.parse(
  tapeBody('https://api.bybit.com/v5/position/list?category=linear&settleCoin=USDT&limit=200&cursor=')!,
).result.list[0];

const positionsBody = (size: string) =>
  JSON.stringify({
    retCode: 0,
    retMsg: 'OK',
    result: {
      list: [{ ...LONG_ROW, side: size === '0' ? '' : 'Buy', size, updatedTime: `${Date.now()}` }],
      nextPageCursor: '',
      category: 'linear',
    },
  });
const EMPTY_ORDERS = '{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":"","category":"linear"}}';
const CANCEL_ALL = '{"retCode":0,"retMsg":"OK","result":{"list":[],"success":"1"},"retExtInfo":{},"time":1760880000000}';
const created = (id: number) =>
  `{"retCode":0,"retMsg":"OK","result":{"orderId":"${id}","orderLinkId":""},"retExtInfo":{},"time":1760880000000}`;

/** Bybit USDT perp whose private stream carries auth acks and nothing else once frames are withheld. */
const venue = (positions: (msSinceClose: number | undefined) => string, onClose?: (push: (frame: string) => void, at: 'close' | 'read') => void) => {
  let pushPrivate: (frame: string) => void = () => {};
  const trace: { at: number; line: string }[] = [];
  let closedAt: number | undefined;
  let orderId = 1000;
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => Date.now(),
    connect: (url, { onOpen, onMessage }) => {
      setTimeout(onOpen, 0);
      if (url.includes('/private')) pushPrivate = onMessage;
      return {
        send: (text) => {
          if (!url.includes('/private')) return;
          const { op, req_id: reqId } = JSON.parse(text) as { op?: string; req_id?: string };
          if (op === 'auth' || op === 'subscribe')
            setTimeout(() => onMessage(JSON.stringify({ op, req_id: reqId, success: true, ret_msg: '' })), 0);
        },
        close: () => {},
      };
    },
    request: async ({ method, url, body }) => {
      const { pathname } = new URL(url);
      const text = typeof body === 'string' ? body : body ? new TextDecoder().decode(body as Uint8Array) : '';
      let reply: string | undefined;
      if (pathname === '/v5/position/list') {
        const settle = new URL(url).searchParams.get('settleCoin');
        reply =
          settle === 'USDT'
            ? positions(closedAt === undefined ? undefined : Date.now() - closedAt)
            : tapeBody(url);
        if (settle === 'USDT' && closedAt !== undefined) onClose?.(pushPrivate, 'read');
        if (settle === 'USDT') trace.push({ at: Date.now(), line: `GET ${pathname} -> size ${JSON.parse(reply!).result.list[0].size}` });
      } else if (pathname === '/v5/order/realtime') reply = EMPTY_ORDERS;
      else if (pathname === '/v5/order/cancel-all') {
        reply = CANCEL_ALL;
        trace.push({ at: Date.now(), line: `POST ${pathname} ${text}` });
      } else if (pathname.startsWith('/v5/order/create')) {
        reply = pathname.endsWith('batch') ? tapeBody(url) : created((orderId += 1));
        if (text.includes('"reduceOnly":true')) {
          closedAt = Date.now();
          onClose?.(pushPrivate, 'close');
        }
        trace.push({ at: Date.now(), line: `POST ${pathname} ${text}` });
      } else {
        reply = tapeBody(url);
        if (method !== 'GET') trace.push({ at: Date.now(), line: `${method} ${pathname} ${text}` });
      }
      return { status: reply ? 200 : 404, headers: '', body: new TextEncoder().encode(reply ?? '{}') };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return { transport, trace };
};

describe('Nuke with the private stream withheld (Bybit USDT perp)', () => {
  let exchange: RustEngineExchange | undefined;
  afterEach(() => {
    exchange?.dispose();
    exchange = undefined;
  });

  const start = async (positions: Parameters<typeof venue>[0], onClose?: Parameters<typeof venue>[1]) => {
    const tape = venue(positions, onClose);
    setWasmEngineLoader(loadNodeEngine);
    setRustEngineTransport(() => tape.transport);
    exchange = new RustEngineExchange('bybit', { key: 'k', secret: 's' }, new DefaultStore());
    await exchange.start();
    await vi.waitFor(() => expect(exchange!.store.loaded.positions).toBe(true));
    return tape;
  };

  const run = async (tape: ReturnType<typeof venue>) => {
    const before = tape.trace.length;
    const result = await exchange!.nuke().then(
      () => 'ok',
      (error: Error) => error.message,
    );
    return { result, lines: tape.trace.slice(before).map((entry) => entry.line) };
  };

  const firstIndex = (lines: string[], pattern: RegExp) => lines.findIndex((line) => pattern.test(line));

  it('the live trace: a bid acked, store orders empty, read long, close 200, read flat; a cancel precedes the close', async () => {
    let bought = false;
    const tape = await start((sinceClose) => {
      if (!bought) return positionsBody('0');
      return positionsBody(sinceClose === undefined ? '13.8' : '0');
    });
    bought = true;
    await exchange!.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 13.8, price: 90000 });
    expect(exchange!.store.orders).toEqual([]);
    expect((await exchange!.fetchPositions()).filter((p) => p.contracts !== 0)).toEqual([]);
    const { result, lines } = await run(tape);
    const cancel = firstIndex(lines, /cancel-all.*BTCUSDT/);
    const close = firstIndex(lines, /create.*"reduceOnly":true/);
    expect(cancel).toBeGreaterThanOrEqual(0);
    expect(cancel).toBeLessThan(close);
    expect(lines.slice(0, close).some((line) => /position\/list -> size 13\.8/.test(line))).toBe(true);
    expect(lines.slice(close).some((line) => /position\/list -> size 0/.test(line))).toBe(true);
    expect(result).toBe('ok');
  }, 30_000);

  it('rejects while the venue stays long 13.8, with a read after the close', async () => {
    const tape = await start(() => positionsBody('13.8'));
    await vi.waitFor(async () => expect((await exchange!.fetchPositions())[0]?.contracts).toBe(13.8));
    const { result, lines } = await run(tape);
    const cancel = firstIndex(lines, /cancel-all.*BTCUSDT/);
    const close = firstIndex(lines, /create.*"reduceOnly":true/);
    expect(cancel).toBeGreaterThanOrEqual(0);
    expect(cancel).toBeLessThan(close);
    expect(lines.slice(close).some((line) => line.includes('/position/list'))).toBe(true);
    expect(result).toMatch(/did not leave the account flat/);
  }, 30_000);

  it('resolves when the read after the close shows flat', async () => {
    const tape = await start((sinceClose) => positionsBody(sinceClose === undefined ? '13.8' : '0'));
    await vi.waitFor(async () => expect((await exchange!.fetchPositions())[0]?.contracts).toBe(13.8));
    const { result, lines } = await run(tape);
    const cancel = firstIndex(lines, /cancel-all.*BTCUSDT/);
    expect(cancel).toBeGreaterThanOrEqual(0);
    expect(cancel).toBeLessThan(firstIndex(lines, /create.*"reduceOnly":true/));
    expect(result).toBe('ok');
  }, 30_000);

  it('a stale flat frame right after the close does not prove flat while the venue stays long', async () => {
    const frame = JSON.stringify({
      id: 'position-1',
      topic: 'position',
      creationTime: Date.now(),
      data: [{ ...LONG_ROW, side: '', size: '0' }],
    });
    const tape = await start(
      () => positionsBody('13.8'),
      (push, at) => at === 'close' && setTimeout(() => push(frame), 50),
    );
    await vi.waitFor(async () => expect((await exchange!.fetchPositions())[0]?.contracts).toBe(13.8));
    const { result } = await run(tape);
    expect(result).toMatch(/did not leave the account flat/);
  }, 30_000);

  it('a read issued under 500 ms after a close never proves flat', async () => {
    const tape = await start((sinceClose) =>
      positionsBody(sinceClose === undefined || sinceClose >= 500 ? '13.8' : '0'),
    );
    await vi.waitFor(async () => expect((await exchange!.fetchPositions())[0]?.contracts).toBe(13.8));
    const { result } = await run(tape);
    expect(result).toMatch(/did not leave the account flat/);
  }, 30_000);
});
