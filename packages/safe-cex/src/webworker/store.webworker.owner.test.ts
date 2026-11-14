import type { StoreWorkerPayload } from './StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

type Context = { postMessage: (message: unknown) => void };
const fixture = vi.hoisted(() => ({
  workers: [] as {
    owner: string;
    context: Context;
    handlePayload: ReturnType<typeof vi.fn>;
    sendSnapshot: ReturnType<typeof vi.fn>;
    exchange: { isDisposed: boolean } | undefined;
  }[],
  starts: new Map<string, Promise<boolean>>(),
  constructions: new Map<string, Promise<void>>(),
  telegram: vi.fn(),
}));

vi.mock('./ExchangeWorker', () => ({
  ExchangeWorker: class {
    exchange: { isDisposed: boolean } | undefined = { isDisposed: false };
    disposed = false;
    isRecovering = false;
    store = { update: vi.fn() };
    owner: string;
    start = vi.fn(async () => {
      const construction = fixture.constructions.get(this.owner);
      if (construction) {
        await construction;
        if (this.disposed) return false;
        this.exchange = { isDisposed: false };
      }
      return fixture.starts.get(this.owner) ?? true;
    });
    retryTimedOutStart = vi.fn(() => false);
    dispose = vi.fn(() => {
      this.disposed = true;
      if (this.exchange) this.exchange.isDisposed = true;
    });
    handlePayload = vi.fn((payload: StoreWorkerPayload) => {
      if (payload.op === 'dispose') this.dispose();
    });
    sendSnapshot = vi.fn(() => this.context.postMessage({ op: 'store', data: { owner: this.owner } }));
    constructor(
      readonly context: Context,
      _accountName: string,
      _exchangeName: string,
      options: { key: string },
      _settings: unknown,
      _metadata: unknown,
      _sessionKey: unknown,
      readonly instanceId?: string,
    ) {
      this.owner = options.key;
      if (fixture.constructions.has(this.owner)) this.exchange = undefined;
      fixture.workers.push(this);
    }
  },
}));
vi.mock('./TelgramWorker', () => ({
  TelegramWorker: class {
    constructor(getExchange: () => unknown, token: string) {
      fixture.telegram(getExchange(), token);
    }
    start() {}
  },
}));

function port(start: typeof import('./store.webworker').default) {
  let listener: (event: { data: unknown }) => unknown = () => undefined;
  const context = {
    postMessage: vi.fn(),
    addEventListener: (_event: string, callback: typeof listener) => {
      listener = callback;
    },
  };
  start(context as unknown as Worker, true);
  return {
    context,
    send: async (data: StoreWorkerPayload) => {
      await listener({ data });
    },
  };
}
function init(owner: string, accountName = 'primary', sessionKey: unknown = `session-${owner}`): StoreWorkerPayload {
  return {
    op: 'init',
    instanceId: owner,
    data: { accountName, exchangeName: 'binance', options: { key: owner }, settings: {}, metadata: {}, sessionKey },
  };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

beforeEach(() => {
  vi.resetModules();
  fixture.workers.length = 0;
  fixture.starts.clear();
  fixture.constructions.clear();
});

describe('SharedWorker session ownership', () => {
  it.each([undefined, 'A'])(
    'preserves identical init completion before exchange construction: %s',
    async (instanceId) => {
      const { default: start } = await import('./store.webworker');
      const a = port(start);
      const construction = deferred<void>();
      fixture.constructions.set('A', construction.promise);
      const payload = { ...init('A'), instanceId };
      payload.data.telegramBotToken = 'synthetic-A-token';
      const pendingInit = a.send(payload);
      expect(fixture.workers[0].exchange).toBeUndefined();
      await a.send(structuredClone(payload));
      expect(fixture.workers).toHaveLength(1);
      expect(fixture.telegram).not.toHaveBeenCalled();
      construction.resolve();
      await pendingInit;
      expect(fixture.workers[0].exchange?.isDisposed).toBe(false);
      expect(fixture.telegram).toHaveBeenCalledExactlyOnceWith(fixture.workers[0].exchange, 'synthetic-A-token');
      const command: StoreWorkerPayload = { op: 'chaser:start', instanceId, data: { id: 'A-chase' } };
      await a.send(command);
      expect(fixture.workers[0].handlePayload).toHaveBeenCalledExactlyOnceWith(command);
    },
  );

  it('supersedes A init before exchange construction when the owner changes to B', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const construction = deferred<void>();
    fixture.constructions.set('A', construction.promise);
    const payload = init('A');
    payload.data.telegramBotToken = 'synthetic-A-token';
    const pendingInit = a.send(payload);
    expect(fixture.workers[0].exchange).toBeUndefined();
    const next = init('B');
    next.data.telegramBotToken = 'synthetic-B-token';
    await a.send(next);
    construction.resolve();
    await pendingInit;
    expect(fixture.workers).toHaveLength(2);
    expect(fixture.workers[0].handlePayload).toHaveBeenCalledExactlyOnceWith({ op: 'dispose' });
    expect(fixture.telegram).toHaveBeenCalledExactlyOnceWith(fixture.workers[1].exchange, 'synthetic-B-token');
    const command: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'B', data: { id: 'B-chase' } };
    await a.send(command);
    expect(fixture.workers[1].handlePayload).toHaveBeenCalledExactlyOnceWith(command);
  });

  it.each([undefined, 'A'])('accepts unstamped commands on an unchanged initial attachment: %s', async (instanceId) => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const initial = { ...init('A'), instanceId };
    await a.send(initial);
    await a.send(initial);
    expect(fixture.workers).toHaveLength(1);
    for (const commandId of [undefined, '']) {
      const command: StoreWorkerPayload = { op: 'chaser:start', instanceId: commandId, data: { id: 'A-chase' } };
      await a.send(command);
      expect(fixture.workers[0].handlePayload).toHaveBeenCalledWith(command);
    }
  });

  it.each([
    ['chaser:start', undefined],
    ['chaser:start', ''],
    ['chaser:terminateAll', undefined],
    ['chaser:terminateAll', ''],
  ] as const)('drops deferred A %s with instanceId %s after B rebinds the port', async (op, instanceId) => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    await a.send(init('A'));
    const release = deferred<void>();
    const old: StoreWorkerPayload = { op, instanceId, data: { id: 'A-chase' } };
    const pending = release.promise.then(() => a.send(old));
    await a.send(init('B'));
    expect(fixture.workers).toHaveLength(2);
    expect(fixture.workers[0].exchange?.isDisposed).toBe(true);
    const current: StoreWorkerPayload = { op, instanceId: 'B', data: { id: 'B-chase' } };
    await a.send(current);
    expect(fixture.workers[1].handlePayload).toHaveBeenCalledExactlyOnceWith(current);
    release.resolve();
    await pending;
    expect(fixture.workers[1].handlePayload).not.toHaveBeenCalledWith(old);
    expect(fixture.workers[1].handlePayload).toHaveBeenCalledExactlyOnceWith(current);
  });

  it.each([undefined, '', 'A'])('refuses an owner rebind without a fresh lifecycle ID: %s', async (instanceId) => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    await a.send(init('A'));
    a.context.postMessage.mockClear();
    await a.send({ ...init('B'), instanceId });
    expect(a.context.postMessage).toHaveBeenCalledWith({ op: 'start-status', data: false });
    expect(fixture.workers).toHaveLength(1);
    expect(fixture.workers[0].exchange?.isDisposed).toBe(false);
    const current: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'A', data: { id: 'A-chase' } };
    await a.send(current);
    expect(fixture.workers[0].handlePayload).toHaveBeenCalledExactlyOnceWith(current);
  });

  it('does not reuse a retired lifecycle ID when the port returns to A', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    await a.send(init('A'));
    const release = deferred<void>();
    const old: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'A', data: { id: 'old-A-chase' } };
    const pending = release.promise.then(() => a.send(old));
    await a.send(init('B'));
    await a.send(init('A'));
    expect(a.context.postMessage).toHaveBeenLastCalledWith(expect.objectContaining({ op: 'warning' }));
    expect(fixture.workers).toHaveLength(2);
    await a.send({ ...init('A'), instanceId: 'fresh-A' });
    expect(fixture.workers).toHaveLength(3);
    release.resolve();
    await pending;
    expect(fixture.workers[2].handlePayload).not.toHaveBeenCalled();
    const current: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'fresh-A', data: { id: 'new-A-chase' } };
    await a.send(current);
    expect(fixture.workers[2].handlePayload).toHaveBeenCalledExactlyOnceWith(current);
  });

  it('retires A when its last port switches to B before A startup settles', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const pendingStart = deferred<boolean>();
    fixture.starts.set('A', pendingStart.promise);
    const pendingInit = a.send(init('A'));
    await a.send(init('B'));
    pendingStart.resolve(true);
    await pendingInit;
    expect(fixture.workers).toHaveLength(2);
    expect(fixture.workers[0].exchange?.isDisposed).toBe(true);
    const late: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'A', data: { id: 'late-A' } };
    await a.send(late);
    expect(fixture.workers[1].handlePayload).not.toHaveBeenCalledWith(late);
    const current: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'B', data: { id: 'B-chase' } };
    await a.send(current);
    expect(fixture.workers[1].handlePayload).toHaveBeenCalledWith(current);
  });

  it('keeps failed-start A ports out of B session until they detach', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const otherA = port(start);
    const b = port(start);
    const pendingStart = deferred<boolean>();
    fixture.starts.set('A', pendingStart.promise);
    const pendingInit = a.send(init('A'));
    await otherA.send(init('A'));
    pendingStart.resolve(false);
    await pendingInit;
    await b.send(init('B'));
    expect(fixture.workers).toHaveLength(1);
    expect(b.context.postMessage).toHaveBeenCalledWith({ op: 'start-status', data: false });
    await a.send({ op: 'dispose', instanceId: 'A' });
    await otherA.send({ op: 'dispose', instanceId: 'A' });
    await b.send(init('B'));
    expect(fixture.workers).toHaveLength(2);
    a.context.postMessage.mockClear();
    otherA.context.postMessage.mockClear();
    fixture.workers[1].context.postMessage({ op: 'store', data: { owner: 'B' } });
    expect(a.context.postMessage).not.toHaveBeenCalled();
    expect(otherA.context.postMessage).not.toHaveBeenCalled();
  });

  it('shares one live worker across two ports of the same session', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const otherA = port(start);
    await a.send(init('A'));
    await otherA.send({ ...init('A'), instanceId: 'other-A' });
    const command: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'other-A', data: { id: 'A-chase' } };
    await otherA.send(command);
    expect(fixture.workers).toHaveLength(1);
    expect(fixture.workers[0].sendSnapshot).toHaveBeenCalledOnce();
    expect(fixture.workers[0].handlePayload).toHaveBeenCalledWith(command);
  });

  it('drops B commands and snapshots when B replaces A during A pending chase', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const otherA = port(start);
    await a.send(init('A'));
    await otherA.send(init('A'));
    const workerA = fixture.workers[0];
    const completion = deferred<void>();
    const pending = completion.promise.then(() =>
      workerA.context.postMessage({ op: 'chaser:complete', data: { id: 'A-chase', filled: 7 } }),
    );
    await a.send({ op: 'chaser:start', instanceId: 'A', data: { id: 'A-chase' } });
    a.context.postMessage.mockClear();
    otherA.context.postMessage.mockClear();
    await a.send(init('B'));
    const bCommands: StoreWorkerPayload[] = [
      { op: 'chaser:start', instanceId: 'B', data: { id: 'B-chase' } },
      { op: 'chaser:terminate', instanceId: 'B', data: { id: 'A-chase' } },
    ];
    for (const command of bCommands) await a.send(command);
    completion.resolve();
    await pending;
    expect(fixture.workers).toHaveLength(1);
    for (const command of bCommands) expect(workerA.handlePayload).not.toHaveBeenCalledWith(command);
    expect(a.context.postMessage).toHaveBeenCalledWith({ op: 'start-status', data: false });
    expect(
      a.context.postMessage.mock.calls.some(([message]) => ['store', 'chaser:complete'].includes(message.op)),
    ).toBe(false);
    expect(otherA.context.postMessage).toHaveBeenCalledWith({
      op: 'chaser:complete',
      data: { id: 'A-chase', filled: 7 },
    });
    expect(workerA.exchange?.isDisposed).toBe(false);
  });

  it.each([undefined, '', null, 42])('refuses reuse without a verifiable session: %s', async (sessionKey) => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const b = port(start);
    await a.send(init('A'));
    const payload = init('B');
    payload.data.sessionKey = sessionKey;
    await b.send(payload);
    await b.send({ op: 'chaser:start', instanceId: 'B', data: { id: 'B-chase' } });
    expect(fixture.workers[0].handlePayload).not.toHaveBeenCalled();
    expect(b.context.postMessage).toHaveBeenCalledWith({ op: 'start-status', data: false });
    expect(fixture.workers[0].sendSnapshot).not.toHaveBeenCalled();
  });

  it('does not let a refused B attachment reinit or dispose A', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const b = port(start);
    await a.send(init('A'));
    await b.send(init('B'));
    await b.send({ op: 'reinit', instanceId: 'B' });
    await b.send({ op: 'dispose', instanceId: 'B' });
    expect(fixture.workers[0].exchange?.isDisposed).toBe(false);
    const command: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'A', data: { id: 'A-chase' } };
    await a.send(command);
    expect(fixture.workers[0].handlePayload).toHaveBeenCalledExactlyOnceWith(command);
  });

  it('drops A stale messages after B replaces a disposed same-name worker', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const b = port(start);
    await a.send(init('A'));
    await a.send({ op: 'dispose', instanceId: 'A' });
    await b.send(init('B'));
    expect(fixture.workers).toHaveLength(2);
    await a.send({ op: 'chaser:start', instanceId: 'A', data: { id: 'late-A' } });
    await a.send({ op: 'reinit', instanceId: 'A' });
    expect(fixture.workers[1].handlePayload).not.toHaveBeenCalled();
    expect(fixture.workers[1].exchange?.isDisposed).toBe(false);
    const command: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'B', data: { id: 'B-chase' } };
    await b.send(command);
    expect(fixture.workers[1].handlePayload).toHaveBeenCalledWith(command);
  });

  it('drops an old init continuation rather than starting A integration on B', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const pendingStart = deferred<boolean>();
    fixture.starts.set('A', pendingStart.promise);
    const payload = init('A');
    payload.data.telegramBotToken = 'synthetic-A-token';
    const pendingInit = a.send(payload);
    await a.send(init('B', 'B:primary'));
    pendingStart.resolve(true);
    await pendingInit;
    expect(fixture.telegram).not.toHaveBeenCalled();
    const command: StoreWorkerPayload = { op: 'chaser:start', instanceId: 'B', data: { id: 'B-chase' } };
    await a.send(command);
    expect(fixture.workers[1].handlePayload).toHaveBeenCalledWith(command);
  });
});

function deferredPublication(ctx: Context, message: unknown) {
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  return { release, done: held.then(() => ctx.postMessage(message)) };
}

describe.each(['account', 'anonymous'] as const)('SharedWorker %s publication ownership', (mode) => {
  const ownerA = (instanceId = 'A') => ({ ...init('A', mode === 'account' ? 'A' : ''), instanceId });
  const ownerB = () => {
    const payload = init('B', mode === 'account' ? 'B' : '');
    payload.data.exchangeName = 'bybit';
    return payload;
  };

  it('preserves deferred delivery to every port still attached to the same owner', async () => {
    const { default: start } = await import('./store.webworker');
    const p = port(start);
    const q = port(start);
    await p.send(ownerA());
    await q.send(ownerA());
    const message = { op: 'store', data: { owner: 'A', publication: 'deferred' } };
    const pending = deferredPublication(fixture.workers[0].context, message);
    pending.release();
    await pending.done;
    expect(p.context.postMessage).toHaveBeenCalledWith(message);
    expect(q.context.postMessage).toHaveBeenCalledWith(message);
  });

  it('keeps a surviving A worker routed to Q after its creating port P rebinds to B', async () => {
    const { default: start } = await import('./store.webworker');
    const p = port(start);
    const q = port(start);
    await p.send(ownerA());
    await q.send(ownerA());
    const message = { op: 'store', data: { owner: 'A', publication: 'deferred' } };
    const pending = deferredPublication(fixture.workers[0].context, message);
    await p.send({ op: 'dispose', instanceId: 'A' });
    await p.send(ownerB());
    pending.release();
    await pending.done;
    expect(p.context.postMessage).not.toHaveBeenCalledWith(message);
    expect(q.context.postMessage).toHaveBeenCalledWith(message);
    const current = { op: 'store', data: { owner: 'B' } };
    fixture.workers[1].context.postMessage(current);
    expect(p.context.postMessage).toHaveBeenCalledWith(current);
    expect(q.context.postMessage).not.toHaveBeenCalledWith(current);
  });

  it('drops deferred output after replacement under the same owner key', async () => {
    const { default: start } = await import('./store.webworker');
    const p = port(start);
    await p.send(ownerA());
    const stale = { op: 'store', data: { generation: 'old' } };
    const pending = deferredPublication(fixture.workers[0].context, stale);
    await p.send({ op: 'dispose', instanceId: 'A' });
    await p.send(ownerA('fresh-A'));
    pending.release();
    await pending.done;
    expect(p.context.postMessage).not.toHaveBeenCalledWith(stale);
    const current = { op: 'store', data: { generation: 'current' } };
    fixture.workers[1].context.postMessage(current);
    expect(p.context.postMessage).toHaveBeenCalledWith(current);
  });

  it.each(['listenOrderBook', 'listenOrderBook:rejected', 'listenOHLCV:rejected'])(
    'never targets another owner for deferred %s when a subscription id is reused',
    async (op) => {
      const { default: start } = await import('./store.webworker');
      const p = port(start);
      const q = port(start);
      await p.send(ownerA());
      await q.send(ownerA());
      const subscription: StoreWorkerPayload = {
        op: 'subscribeOrderBook',
        instanceId: 'A',
        data: { subId: 'reused', symbol: 'BTCUSDT' },
      };
      await q.send(subscription);
      const current = { op, data: { subId: 'reused', owner: 'A', update: 1 } };
      fixture.workers[0].context.postMessage(current);
      expect(q.context.postMessage).toHaveBeenCalledWith(current);
      expect(p.context.postMessage).not.toHaveBeenCalledWith(current);
      const stale = { op, data: { subId: 'reused', owner: 'A', update: 2 } };
      const pending = deferredPublication(fixture.workers[0].context, stale);
      await p.send({ op: 'dispose', instanceId: 'A' });
      await p.send(ownerB());
      await p.send({ ...subscription, instanceId: 'B' });
      pending.release();
      await pending.done;
      expect(p.context.postMessage).not.toHaveBeenCalledWith(stale);
    },
  );
});
