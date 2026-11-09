import type { ExchangeName } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import startWorkerContext, { anonymousExchangeWorkers, exchangeWorkers } from './store.webworker';

const workers = vi.hoisted(() => [] as Array<{ ctx: Worker; dispose: () => void }>);
vi.mock('./ExchangeWorker', () => ({
  ExchangeWorker: class {
    exchange = { isDisposed: false };
    store = { update: vi.fn() };
    constructor(readonly ctx: Worker) {
      workers.push(this);
    }
    start = async () => true;
    sendSnapshot = vi.fn();
    dispose = () => {
      this.exchange.isDisposed = true;
    };
    handlePayload = (payload: { op: string }) => {
      if (payload.op === 'dispose') this.dispose();
    };
  },
}));

const clients: Array<{ send: (data: unknown) => Promise<void> }> = [];
function client() {
  let receive!: (event: { data: unknown }) => Promise<void>;
  const messages: unknown[] = [];
  const ctx = {
    postMessage: (message: unknown) => messages.push(message),
    addEventListener: (_type: string, listener: typeof receive) => (receive = listener),
  };
  startWorkerContext(ctx as unknown as Worker, true);
  const port = { messages, send: (data: unknown) => receive({ data }) };
  clients.push(port);
  return port;
}

function init(accountName: string, exchangeName: ExchangeName = 'binance') {
  return { op: 'init', data: { accountName, exchangeName, options: {}, settings: {}, metadata: {}, store: {} } };
}

function deferredPublication(ctx: Worker, message: unknown) {
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  return { release, done: held.then(() => ctx.postMessage(message)) };
}

afterEach(async () => {
  for (const port of clients.splice(0)) await port.send({ op: 'dispose' });
  for (const worker of workers.splice(0)) worker.dispose();
  for (const key of Object.keys(exchangeWorkers)) delete exchangeWorkers[key];
  for (const key of Object.keys(anonymousExchangeWorkers)) delete anonymousExchangeWorkers[key];
});

describe.each(['account', 'anonymous'] as const)('SharedWorker %s publication ownership', (mode) => {
  const ownerA = () => init(mode === 'account' ? 'A' : '', 'binance');
  const ownerB = () => init(mode === 'account' ? 'B' : '', 'bybit');

  it('preserves deferred delivery to every port still attached to the same owner', async () => {
    const p = client();
    const q = client();
    await p.send(ownerA());
    await q.send(ownerA());
    const message = { op: 'store', data: { owner: 'A' } };
    const pending = deferredPublication(workers[0].ctx, message);
    pending.release();
    await pending.done;
    expect(p.messages).toContainEqual(message);
    expect(q.messages).toContainEqual(message);
  });

  it('keeps a surviving A worker routed to Q after its creating port P rebinds to B', async () => {
    const p = client();
    const q = client();
    await p.send(ownerA());
    await q.send(ownerA());
    const message = { op: 'store', data: { owner: 'A' } };
    const pending = deferredPublication(workers[0].ctx, message);
    await p.send({ op: 'dispose' });
    await p.send(ownerB());
    pending.release();
    await pending.done;
    expect(p.messages).not.toContainEqual(message);
    expect(q.messages).toContainEqual(message);
    const current = { op: 'store', data: { owner: 'B' } };
    workers[1].ctx.postMessage(current);
    expect(p.messages).toContainEqual(current);
    expect(q.messages).not.toContainEqual(current);
  });

  it('drops deferred output after replacement under the same owner key', async () => {
    const p = client();
    await p.send(ownerA());
    const stale = { op: 'store', data: { generation: 'old' } };
    const pending = deferredPublication(workers[0].ctx, stale);
    await p.send({ op: 'dispose' });
    await p.send(ownerA());
    pending.release();
    await pending.done;
    expect(p.messages).not.toContainEqual(stale);
    const current = { op: 'store', data: { generation: 'current' } };
    workers[1].ctx.postMessage(current);
    expect(p.messages).toContainEqual(current);
  });

  it('never targets another owner when a subscription id is reused there during the deferral', async () => {
    const p = client();
    const q = client();
    await p.send(ownerA());
    await q.send(ownerA());
    const subscription = { op: 'subscribeOrderBook', data: { subId: 'reused', symbol: 'BTCUSDT' } };
    await q.send(subscription);
    const current = { op: 'listenOrderBook', data: { subId: 'reused', owner: 'A', update: 1 } };
    workers[0].ctx.postMessage(current);
    expect(q.messages).toContainEqual(current);
    expect(p.messages).not.toContainEqual(current);
    const stale = { op: 'listenOrderBook', data: { subId: 'reused', owner: 'A', update: 2 } };
    const pending = deferredPublication(workers[0].ctx, stale);
    await p.send({ op: 'dispose' });
    await p.send(ownerB());
    await p.send(subscription);
    pending.release();
    await pending.done;
    expect(p.messages).not.toContainEqual(stale);
  });
});
