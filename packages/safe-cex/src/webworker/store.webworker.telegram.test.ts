import type { StoreWorkerPayload } from './StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

const fixture = vi.hoisted(() => ({
  workers: [] as {
    exchange: { isDisposed: boolean } | undefined;
    isRecovering: boolean;
    onStartRetrySucceeded?: () => void;
  }[],
  constructions: new Map<string, Promise<void>>(),
  starts: new Map<string, Promise<void>>(),
  retries: new Set<string>(),
  telegram: vi.fn(),
}));

vi.mock('./ExchangeWorker', () => ({
  ExchangeWorker: class {
    exchange: { isDisposed: boolean } | undefined;
    store = { update: vi.fn() };
    disposed = false;
    isRecovering = false;
    onStartRetrySucceeded?: () => void;
    constructor(
      _context: Worker,
      readonly accountName: string,
    ) {
      if (!fixture.constructions.has(accountName)) this.exchange = { isDisposed: false };
      fixture.workers.push(this);
    }
    async start() {
      await fixture.constructions.get(this.accountName);
      if (this.disposed) return false;
      this.exchange ??= { isDisposed: false };
      await fixture.starts.get(this.accountName);
      return !this.disposed && !fixture.retries.has(this.accountName);
    }
    retryTimedOutStart() {
      if (!fixture.retries.has(this.accountName)) return false;
      this.isRecovering = true;
      this.exchange = { isDisposed: true };
      return true;
    }
    sendSnapshot = vi.fn();
    dispose() {
      this.disposed = true;
      if (this.exchange) this.exchange.isDisposed = true;
    }
    handlePayload(payload: StoreWorkerPayload) {
      if (payload.op === 'dispose') this.dispose();
    }
  },
}));
vi.mock('./TelgramWorker', () => ({
  TelegramWorker: class {
    constructor(
      readonly getExchange: () => unknown,
      readonly token: string,
    ) {}
    start() {
      fixture.telegram(this.getExchange(), this.token);
    }
  },
}));

function port(start: typeof import('./store.webworker').default, shared = true) {
  let listener: (event: { data: unknown }) => unknown = () => undefined;
  start(
    {
      postMessage: vi.fn(),
      addEventListener: (_event: string, callback: typeof listener) => (listener = callback),
    } as unknown as Worker,
    shared,
  );
  return { send: async (data: StoreWorkerPayload) => void (await listener({ data })) };
}

function init(accountName = 'A', instanceId: string | undefined = accountName): StoreWorkerPayload {
  return {
    op: 'init',
    instanceId,
    data: {
      accountName,
      exchangeName: 'binance',
      sessionKey: `session-${accountName}`,
      options: {},
      settings: {},
      metadata: {},
      telegramBotToken: `synthetic-${accountName}-token`,
    },
  };
}

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => (resolve = done));
  return { promise, resolve };
}

beforeEach(() => {
  vi.resetModules();
  fixture.workers.length = 0;
  fixture.constructions.clear();
  fixture.starts.clear();
  fixture.retries.clear();
});

describe('Telegram integration ownership', () => {
  it.each([undefined, 'A'])('starts once across identical same-port init: %s', async (instanceId) => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const payload = { ...init(), instanceId };
    await a.send(payload);
    await a.send(structuredClone(payload));
    expect(fixture.workers).toHaveLength(1);
    expect(fixture.telegram).toHaveBeenCalledExactlyOnceWith(fixture.workers[0].exchange, 'synthetic-A-token');
  });

  it.each(['construction', 'startup'] as const)(
    'starts once across identical init during deferred %s',
    async (phase) => {
      const { default: start } = await import('./store.webworker');
      const a = port(start);
      const gate = deferred();
      (phase === 'construction' ? fixture.constructions : fixture.starts).set('A', gate.promise);
      const payload = init();
      const pending = a.send(payload);
      await a.send(structuredClone(payload));
      gate.resolve();
      await pending;
      expect(fixture.workers).toHaveLength(1);
      expect(fixture.telegram).toHaveBeenCalledExactlyOnceWith(fixture.workers[0].exchange, 'synthetic-A-token');
    },
  );

  it('starts once for the same owner shared by two ports', async () => {
    const { default: start } = await import('./store.webworker');
    await port(start).send(init());
    await port(start).send(init('A', 'second-port'));
    expect(fixture.workers).toHaveLength(1);
    expect(fixture.telegram).toHaveBeenCalledExactlyOnceWith(fixture.workers[0].exchange, 'synthetic-A-token');
  });

  it.each(['account', 'anonymous'] as const)(
    'preserves deferred startup for a surviving shared %s owner',
    async (mode) => {
      const { default: start, exchangeWorkers, anonymousExchangeWorkers } = await import('./store.webworker');
      const p = port(start);
      const q = port(start);
      const a = init(mode === 'account' ? 'A' : '', 'P-A');
      const gate = deferred();
      fixture.constructions.set(a.data.accountName, gate.promise);
      const pending = p.send(a);
      expect(fixture.workers[0].exchange).toBeUndefined();
      const survivingInit = { ...a, instanceId: 'Q-A' };
      await q.send(survivingInit);
      await p.send({ op: 'dispose', instanceId: 'P-A' });
      const b = init('B');
      await p.send({ ...b, data: { ...b.data, telegramBotToken: undefined } });
      const registryA = mode === 'account' ? exchangeWorkers.A : anonymousExchangeWorkers.binance;
      expect(registryA).toBe(fixture.workers[0]);
      expect(fixture.workers).toHaveLength(2);
      expect(fixture.telegram).not.toHaveBeenCalled();
      gate.resolve();
      await pending;
      expect(registryA.exchange?.isDisposed).toBe(false);
      expect(fixture.telegram).toHaveBeenCalledExactlyOnceWith(registryA.exchange, a.data.telegramBotToken);
      await q.send(structuredClone(survivingInit));
      expect(fixture.telegram).toHaveBeenCalledTimes(1);
      await p.send(b);
      expect(fixture.telegram).toHaveBeenCalledTimes(2);
      expect(fixture.telegram).toHaveBeenLastCalledWith(fixture.workers[1].exchange, 'synthetic-B-token');
    },
  );

  it('does not count another port that has rebound during deferred construction', async () => {
    const { default: start } = await import('./store.webworker');
    const p = port(start);
    const q = port(start);
    const gate = deferred();
    fixture.constructions.set('A', gate.promise);
    const pending = p.send(init());
    await q.send(init('A', 'Q-A'));
    await p.send({ op: 'dispose', instanceId: 'A' });
    for (const [client, owner] of [
      [p, 'B'],
      [q, 'C'],
    ] as const) {
      const next = init(owner);
      await client.send({ ...next, data: { ...next.data, telegramBotToken: undefined } });
    }
    gate.resolve();
    await pending;
    expect(fixture.workers).toHaveLength(3);
    expect(fixture.telegram).not.toHaveBeenCalled();
  });

  it.each([true, false])('checks the surviving owner when startup retry completes: survivor=%s', async (survivor) => {
    const { default: start } = await import('./store.webworker');
    const p = port(start);
    const q = port(start);
    fixture.retries.add('A');
    await p.send(init());
    const workerA = fixture.workers[0];
    expect(workerA.onStartRetrySucceeded).toBeTypeOf('function');
    if (survivor) await q.send(init('A', 'Q-A'));
    await p.send({ op: 'dispose', instanceId: 'A' });
    const b = init('B');
    await p.send({ ...b, data: { ...b.data, telegramBotToken: undefined } });
    // Deliver even a canceled retry's saved callback to exercise the ownership guard.
    workerA.exchange = { isDisposed: false };
    workerA.isRecovering = false;
    workerA.onStartRetrySucceeded!();
    workerA.onStartRetrySucceeded!();
    if (survivor) {
      expect(fixture.telegram).toHaveBeenCalledExactlyOnceWith(workerA.exchange, 'synthetic-A-token');
    } else {
      expect(fixture.telegram).not.toHaveBeenCalled();
    }
    await p.send(b);
    expect(fixture.telegram).toHaveBeenCalledTimes(survivor ? 2 : 1);
    expect(fixture.telegram).toHaveBeenLastCalledWith(fixture.workers[1].exchange, 'synthetic-B-token');
  });

  it('does not consume the start when the first init has no token', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    const payload = init();
    await a.send({ ...payload, data: { ...payload.data, telegramBotToken: undefined } });
    expect(fixture.telegram).not.toHaveBeenCalled();
    await a.send(payload);
    await a.send(structuredClone(payload));
    expect(fixture.telegram).toHaveBeenCalledExactlyOnceWith(fixture.workers[0].exchange, 'synthetic-A-token');
  });

  it('starts again for a replacement worker under the same account name', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
    await a.send(init());
    await a.send({ op: 'dispose', instanceId: 'A' });
    await a.send(init('A', 'replacement'));
    expect(fixture.workers).toHaveLength(2);
    expect(fixture.telegram).toHaveBeenCalledTimes(2);
    expect(fixture.telegram).toHaveBeenLastCalledWith(fixture.workers[1].exchange, 'synthetic-A-token');
  });

  it.each([true, false])(
    'drops A startup when its only port changes to B during construction: shared=%s',
    async (shared) => {
      const { default: start } = await import('./store.webworker');
      const a = port(start, shared);
      const gate = deferred();
      fixture.constructions.set('A', gate.promise);
      const pending = a.send(init());
      const b = init('B');
      await a.send({ ...b, data: { ...b.data, telegramBotToken: undefined } });
      gate.resolve();
      await pending;
      expect(fixture.workers).toHaveLength(2);
      expect(fixture.telegram).not.toHaveBeenCalled();
      await a.send(b);
      expect(fixture.telegram).toHaveBeenCalledExactlyOnceWith(fixture.workers[1].exchange, 'synthetic-B-token');
    },
  );
});
