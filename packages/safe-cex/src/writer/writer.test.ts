import type { Order, Position } from '../types';
import type { WriterReply, WriterRequest } from './writerRpc';

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { MarginType, OrderSide, OrderStatus, OrderType, PositionSide } from '../types';
import { buildWriteContext } from './writeContext';
import { ChannelWriterHost, LocalWriterHost, serveWriter } from './writerHost';
import { isWriterReply, isWriterRequest } from './writerRpc';

const fixtures = JSON.parse(
  readFileSync(resolve(__dirname, '../../../../sdk-rs/engine/schema/writer-rpc/fixtures.json'), 'utf8'),
) as { requests: unknown[]; replies: unknown[]; invalid: unknown[] };

const order = (id: string, symbol: string, extra: Partial<Order> = {}): Order => ({
  id,
  status: OrderStatus.Open,
  symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 0.001,
  filled: 0,
  remaining: 0.001,
  reduceOnly: false,
  lastUpdate: 0,
  ...extra,
});

const position = (symbol: string, side: PositionSide, extra: Partial<Position> = {}): Position => ({
  id: `${symbol}:${side}`,
  symbol,
  side,
  entryPrice: 60_000,
  notional: 60,
  leverage: 10,
  unrealizedPnl: 0,
  contracts: 0.001,
  liquidationPrice: 0,
  ...extra,
});

const store = {
  orders: [order('o-1', 'BTCUSDT', { clientOrderId: 'c-1' }), order('o-2', 'ETHUSDT')],
  positions: [
    position('BTCUSDT', PositionSide.Long, { marginType: MarginType.Cross }),
    position('ETHUSDT', PositionSide.Short),
  ],
  options: { isHedged: true },
};

describe('writer RPC fixtures', () => {
  it('accepts every canonical request and reply', () => {
    for (const request of fixtures.requests) expect(isWriterRequest(request), JSON.stringify(request)).toBe(true);
    for (const reply of fixtures.replies) expect(isWriterReply(reply), JSON.stringify(reply)).toBe(true);
  });

  it('refuses every invalid message', () => {
    for (const message of fixtures.invalid) {
      expect(isWriterRequest(message) || isWriterReply(message), JSON.stringify(message)).toBe(false);
    }
  });

  it('builds the fixture contexts from a store', () => {
    const [place, amend, cancel] = fixtures.requests as WriterRequest[];
    const fixtureStore = {
      orders: [order('o-1', 'BTCUSDT', { clientOrderId: 'c-1' })],
      positions: [position('BTCUSDT', PositionSide.Long, { contracts: 0.002, marginType: MarginType.Cross })],
      options: { isHedged: true },
    };
    expect(buildWriteContext(fixtureStore, place.command)).toEqual(place.context);
    expect(buildWriteContext(fixtureStore, amend.command)).toEqual({
      ...amend.context,
      positions: place.context.positions,
      settings: place.context.settings,
    });
    expect(buildWriteContext(fixtureStore, cancel.command).orders).toEqual([
      { ...cancel.context.orders![0], clientOrderId: 'c-1' },
    ]);
  });
});

describe('buildWriteContext', () => {
  it('carries only the orders a write names and the symbols it touches', () => {
    const context = buildWriteContext(store, {
      op: 'cancelOrders',
      ids: ['o-2'],
      orders: [{ id: 'o-2', symbol: 'ETHUSDT' }],
    });
    expect(context.orders?.map((row) => row.id)).toEqual(['o-2']);
    expect(context.positions).toEqual([{ symbol: 'ETHUSDT', side: 'short', contracts: 0.001 }]);
    expect(context.settings).toEqual({ ETHUSDT: { hedged: true } });
  });

  it('carries every order of a symbol for a symbol cancel', () => {
    const context = buildWriteContext(store, { op: 'cancelSymbolOrders', symbol: 'BTCUSDT' });
    expect(context.orders?.map((row) => row.id)).toEqual(['o-1']);
  });

  it('carries the whole account for an account-wide write', () => {
    const context = buildWriteContext(store, { op: 'cancelSymbolOrders', symbol: '' });
    expect(context.orders?.map((row) => row.id)).toEqual(['o-1', 'o-2']);
    expect(Object.keys(context.settings ?? {})).toEqual(['BTCUSDT', 'ETHUSDT']);
  });

  it('is empty for a market refresh', () => {
    expect(buildWriteContext(store, { op: 'refreshMarkets' })).toEqual({});
  });
});

const request = (id: string, account = 'acct'): WriterRequest => ({
  kind: 'writerRequest',
  id,
  account,
  command: { op: 'cancelSymbolOrders', symbol: 'BTCUSDT' },
  context: {},
  marks: { called: 1 },
});

describe('writer hosts', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('runs a local write once with its context and replies with the result', async () => {
    const engine = vi.fn(async () => ['o-9']);
    const host = new LocalWriterHost('acct', engine);
    const reply = await host.write({ ...request('w-1'), context: { settings: { BTCUSDT: { hedged: true } } } });
    expect(engine).toHaveBeenCalledExactlyOnceWith({
      op: 'cancelSymbolOrders',
      symbol: 'BTCUSDT',
      context: { settings: { BTCUSDT: { hedged: true } } },
    });
    expect(reply).toEqual({ kind: 'writerReply', id: 'w-1', ok: true, result: ['o-9'], marks: { called: 1 } });
  });

  it('refuses a write for another account without running it', async () => {
    const engine = vi.fn(async () => null);
    const reply = await new LocalWriterHost('acct', engine).write(request('w-1', 'other'));
    expect(engine).not.toHaveBeenCalled();
    expect(reply).toMatchObject({ ok: false, error: { kind: 'invalidInput', outcome: 'notSent' } });
  });

  it('carries an engine failure with its outcome', async () => {
    const failure = Object.assign(new Error('order not found'), { kind: 'rejected', outcome: 'rejected' });
    const reply = await new LocalWriterHost('acct', async () => Promise.reject(failure)).write(request('w-1'));
    expect(reply).toMatchObject({
      ok: false,
      error: { message: 'order not found', kind: 'rejected', outcome: 'rejected' },
    });
  });

  it('round-trips a write across a channel, stamping received on the far side', async () => {
    const engine = vi.fn(async () => null);
    let client: ChannelWriterHost;
    const serve = serveWriter(
      (account) => (account === 'acct' ? new LocalWriterHost('acct', engine) : undefined),
      (reply) => queueMicrotask(() => client.receive(JSON.parse(JSON.stringify(reply)))),
      () => 2,
    );
    client = new ChannelWriterHost(
      (outbound) => queueMicrotask(() => serve(JSON.parse(JSON.stringify(outbound)))),
      1_000,
    );
    const reply = await client.write(request('w-1'));
    expect(engine).toHaveBeenCalledOnce();
    expect(reply).toEqual({
      kind: 'writerReply',
      id: 'w-1',
      ok: true,
      result: null,
      marks: { called: 1, received: 2 },
    });
  });

  it('answers an unknown account and a malformed request as not sent', () => {
    const replies: WriterReply[] = [];
    const serve = serveWriter(
      () => undefined,
      (reply) => replies.push(reply),
      () => 2,
    );
    expect(serve(request('w-1', 'nobody'))).toBe(true);
    expect(
      serve({ kind: 'writerRequest', id: 'w-2', account: 'acct', command: { op: 'fetchOrders' }, context: {} }),
    ).toBe(true);
    expect(serve({ op: 'callMethod' })).toBe(false);
    expect(replies.map((reply) => [reply.id, reply.ok, !reply.ok && reply.error.outcome])).toEqual([
      ['w-1', false, 'notSent'],
      ['w-2', false, 'notSent'],
    ]);
  });

  it('reports a write whose reply never comes as an unknown outcome', async () => {
    vi.useFakeTimers();
    const client = new ChannelWriterHost(() => {}, 500);
    const pending = client.write(request('w-1'));
    await vi.advanceTimersByTimeAsync(500);
    expect(await pending).toMatchObject({
      ok: false,
      error: { code: 'COMMAND_TIMEOUT', command: 'cancelSymbolOrders', outcome: 'unknown' },
    });
  });

  it('settles every pending write as unknown when the channel closes', async () => {
    const client = new ChannelWriterHost(() => {}, 60_000);
    const pending = client.write(request('w-1'));
    client.dispose();
    expect(await pending).toMatchObject({ ok: false, error: { outcome: 'unknown' } });
  });

  it('refuses an undeliverable request as not sent', async () => {
    const client = new ChannelWriterHost(() => {
      throw new Error('port closed');
    }, 60_000);
    expect(await client.write(request('w-1'))).toMatchObject({ ok: false, error: { outcome: 'notSent' } });
  });
});
