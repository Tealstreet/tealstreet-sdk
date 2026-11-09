import type { StoreWorkerPayload } from './StoreWorkerPayload';

import { beforeEach, describe, expect, it, vi } from 'vitest';

const fixture = vi.hoisted(() => ({
  workers: [] as { exchange: { isDisposed: boolean } | undefined }[],
  constructions: new Map<string, Promise<void>>(),
  starts: new Map<string, Promise<void>>(),
  telegram: vi.fn(),
}));

vi.mock('./ExchangeWorker', () => ({
  ExchangeWorker: class {
    exchange: { isDisposed: boolean } | undefined;
    store = { update: vi.fn() };
    disposed = false;
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
      return !this.disposed;
    }
    retryTimedOutStart = () => false;
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

function port(start: typeof import('./store.webworker').default) {
  let listener: (event: { data: unknown }) => unknown = () => undefined;
  start(
    {
      postMessage: vi.fn(),
      addEventListener: (_event: string, callback: typeof listener) => (listener = callback),
    } as unknown as Worker,
    true,
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

  it('drops A integration startup if the port changes to B during construction', async () => {
    const { default: start } = await import('./store.webworker');
    const a = port(start);
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
  });
});
