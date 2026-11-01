import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { consumerFrame } from '../__tests__/cutover/consumerFrame';
import { engineCapabilities } from '../__tests__/fixtures/engineCapabilities';
import { setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { OrderSide, OrderType } from '../types';
import { virtualClock } from '../utils/virtualClock';
import { setMainAcceptsFrames } from './frameForwarding';
import startWorkerContext, { exchangeWorkers } from './store.webworker';
import { setTwapLockManager } from './twap/twapLock';

const telegram = vi.hoisted(() => ({ started: [] as Array<() => { isDisposed: boolean }> }));
vi.mock('./TelgramWorker', () => ({
  TelegramWorker: class {
    constructor(private readonly getExchange: () => { isDisposed: boolean }) {}
    start() {
      telegram.started.push(this.getExchange);
    }
  },
}));

/** An engine whose account never loads, as on a link too slow for its start bound. */
let loads = false;
const built: SlowEngine[] = [];
class SlowEngine implements WasmEngineLike {
  commands: Array<Record<string, unknown>> = [];
  private sent = false;
  constructor(readonly venue = 'binance') {
    built.push(this);
  }
  capabilities = () => engineCapabilities(this.venue);
  start = () => '[]';
  command = (json: string) => {
    const command = JSON.parse(json);
    this.commands.push(command);
    return command.command === undefined
      ? '[]'
      : JSON.stringify([{ op: 'done', command: command.command, result: '[]' }]);
  };
  takeFrame = (_now: number, marketOnly: boolean) => {
    if (!loads || this.sent || marketOnly) return undefined;
    this.sent = true;
    return new Uint8Array(
      consumerFrame({
        markets: [],
        tickers: [],
        orders: [],
        positions: [],
        balance: { total: 1, free: 1, used: 0, upnl: 0 },
        loaded: { markets: true, tickers: true, orders: true, positions: true, balance: true },
      }),
    );
  };
  snapshotFrame = () => new Uint8Array(consumerFrame({ markets: [], tickers: [] }));
  wsOpen = () => '[]';
  wsMessage = this.wsOpen;
  wsClose = this.wsOpen;
  wsRouteExhausted = this.wsOpen;
  httpStart = this.wsOpen;
  httpChunk = this.wsOpen;
  httpEnd = this.wsOpen;
  httpFailed = this.wsOpen;
  timer = this.wsOpen;
}
const transport: Transport = {
  now: () => Date.now(),
  request: async () => {
    throw new Error('No fixture HTTP');
  },
  connect: () => {
    throw new Error('No fixture sockets');
  },
  setTimeout: (callback, ms) => setTimeout(callback, ms),
  clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

const ACCOUNT = 'slow-link';
const context = () => {
  let receive!: (event: { data: unknown }) => Promise<void>;
  const messages: Array<{ op: string; data: any }> = [];
  const ctx = {
    postMessage: (message: any) => messages.push(message),
    addEventListener: (_kind: string, listener: typeof receive) => {
      receive = listener;
    },
  };
  startWorkerContext(ctx as unknown as Worker, false);
  return { messages, send: (data: unknown) => receive({ data }) };
};
const init = {
  op: 'init',
  data: {
    accountName: ACCOUNT,
    exchangeName: 'binance',
    options: { engine: 'rust', key: 'k', secret: 's' },
    settings: {},
    metadata: {},
    sessionKey: 'fixture',
    store: {},
  },
};
const buy = (requestId: string) => ({
  op: 'callMethod',
  data: {
    requestId,
    method: 'placeOrder',
    args: [{ symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Market, amount: 1 }],
  },
});
const read = (requestId: string) => ({
  op: 'callMethod',
  data: { requestId, method: 'fetchHistoricalFills', args: [{ from: 0, to: 1 }] },
});
const reply = (messages: Array<{ op: string; data: any }>, requestId: string) =>
  messages.filter((message) => message.op === 'callMethod:done' && message.data.requestId === requestId);
const reportedStartFailure = (messages: Array<{ op: string; data: any }>) =>
  messages.some((message) => message.op === 'error' && /Error starting exchange/.test(message.data[0]));

beforeEach(() => {
  setTwapLockManager(null);
  vi.useFakeTimers();
  loads = false;
  built.length = 0;
  telegram.started.length = 0;
  vi.spyOn(virtualClock, 'start').mockResolvedValue();
  setMainAcceptsFrames(true);
  setRustEngineTransport(() => transport);
  setWasmEngineLoader(async () => ({ WasmEngine: SlowEngine }));
});
afterEach(async () => {
  exchangeWorkers[ACCOUNT]?.dispose();
  delete exchangeWorkers[ACCOUNT];
  setMainAcceptsFrames(false);
  vi.useRealTimers();
});

describe('a Rust start on a slow link', () => {
  it('is not a failed start at 30 s, and a load at 60 s succeeds', async () => {
    const { messages, send } = context();
    void send(init);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(reportedStartFailure(messages)).toBe(false);
    loads = true;
    await vi.advanceTimersByTimeAsync(1_000);
    expect(exchangeWorkers[ACCOUNT].exchange?.isDisposed).toBe(false);
    expect(reportedStartFailure(messages)).toBe(false);
  });

  it('still refuses a write queued for startup at its 30 s bound', async () => {
    const { messages, send } = context();
    void send(init);
    await vi.advanceTimersByTimeAsync(1_000);
    await send(buy('queued'));
    await vi.advanceTimersByTimeAsync(30_000);
    expect(reply(messages, 'queued')).toHaveLength(1);
    expect(reply(messages, 'queued')[0].data.result).toMatchObject({ outcome: 'notSent' });
  });

  it('answers a write and a read held at the timeout, the write as notSent', async () => {
    const { messages, send } = context();
    void send(init);
    await vi.advanceTimersByTimeAsync(119_000);
    await send(buy('held-write'));
    await send(read('held-read'));
    await vi.advanceTimersByTimeAsync(1_100);
    expect(reportedStartFailure(messages)).toBe(true);
    expect(reply(messages, 'held-write')).toHaveLength(1);
    expect(reply(messages, 'held-write')[0].data).toMatchObject({ success: false, result: { outcome: 'notSent' } });
    expect(reply(messages, 'held-read')).toHaveLength(1);
    expect(reply(messages, 'held-read')[0].data.success).toBe(false);
  });

  it('answers calls during the retry wait as notSent and sends nothing', async () => {
    const { messages, send } = context();
    void send(init);
    await vi.advanceTimersByTimeAsync(121_000);
    await send(buy('waiting-write'));
    expect(reply(messages, 'waiting-write')[0].data).toMatchObject({ success: false, result: { outcome: 'notSent' } });
    expect(built.flatMap((engine) => engine.commands).some((command) => command.op === 'placeOrders')).toBe(false);
  });

  it('tells the user and starts over with backoff until the account loads', async () => {
    const { messages, send } = context();
    void send(init);
    await vi.advanceTimersByTimeAsync(120_100);
    expect(messages.filter((message) => message.op === 'warning').at(-1)?.data[0]).toContain('retrying in 10s');
    expect(built).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(built).toHaveLength(2);
    loads = true;
    await vi.advanceTimersByTimeAsync(1_000);
    expect(exchangeWorkers[ACCOUNT].isRecovering).toBe(false);
    await send(buy('after-recovery'));
    await vi.advanceTimersByTimeAsync(1_000);
    expect(reply(messages, 'after-recovery')[0].data.success).toBe(true);
    expect(messages.some((message) => message.op === 'ws-reconnected')).toBe(true);
  });

  it('gives up visibly once the retries run out, and still answers', async () => {
    const { messages, send } = context();
    void send(init);
    await vi.advanceTimersByTimeAsync(5 * 120_000 + 130_000 + 1_000);
    expect(built).toHaveLength(5);
    expect(messages.filter((message) => message.op === 'warning').at(-1)?.data[0]).toContain('recovery failed');
    await send(read('late'));
    expect(reply(messages, 'late')[0].data.success).toBe(false);
    await vi.advanceTimersByTimeAsync(600_000);
    expect(built).toHaveLength(5);
  });
});

describe('a call that beats the exchange into being', () => {
  const metadata = (requestId: string) => ({
    op: 'callMethod',
    data: { requestId, method: 'updateSymbolMetadata', args: ['BTCUSDT'] },
  });

  it('is answered under the engine bounds, not held for the whole start', async () => {
    const { messages, send } = context();
    const module = { WasmEngine: SlowEngine };
    let finishLoad = () => {};
    const loader = new Promise<void>((resolve) => {
      finishLoad = resolve;
    });
    setWasmEngineLoader(async () => {
      await loader;
      return module;
    });
    const clock = vi.spyOn(virtualClock, 'start');
    let clockDone = () => {};
    clock.mockReturnValueOnce(new Promise<void>((resolve) => (clockDone = resolve)));
    void send(init);
    await vi.advanceTimersByTimeAsync(1_000);
    await send(metadata('held-metadata'));
    await send(buy('held-write'));
    await send(read('held-read'));
    clockDone();
    finishLoad();
    await vi.advanceTimersByTimeAsync(100);
    expect(reply(messages, 'held-metadata')[0].data.success).toBe(true);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(reply(messages, 'held-write')[0].data.result).toMatchObject({ outcome: 'notSent' });
    expect(reportedStartFailure(messages)).toBe(false);
    expect(reply(messages, 'held-read')).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(90_000);
    expect(reply(messages, 'held-read')).toHaveLength(1);
  });
});

describe('an exchange that never comes into being', () => {
  it('refuses a held write and read as notSent at 30 s and sends nothing', async () => {
    vi.spyOn(virtualClock, 'start').mockReturnValueOnce(new Promise<void>(() => {}));
    const { messages, send } = context();
    void send(init);
    await vi.advanceTimersByTimeAsync(1_000);
    await send(buy('stuck-write'));
    await send(read('stuck-read'));
    await vi.advanceTimersByTimeAsync(29_000);
    expect(reply(messages, 'stuck-write')).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1_100);
    expect(reply(messages, 'stuck-write')[0].data.result).toMatchObject({ outcome: 'notSent' });
    expect(reply(messages, 'stuck-read')[0].data.success).toBe(false);
    expect(built.flatMap((engine) => engine.commands).some((command) => command.op === 'placeOrders')).toBe(false);
  });

  it('a write once the exchange exists and the private API failed gets its 30 s refusal, not the command timeout', async () => {
    const { messages, send } = context();
    void send(init);
    await vi.advanceTimersByTimeAsync(9_000);
    await send(buy('offline-write'));
    await send(read('offline-read'));
    await vi.advanceTimersByTimeAsync(30_100);
    expect(reply(messages, 'offline-write')[0].data.result).toMatchObject({ outcome: 'notSent' });
    expect(built.flatMap((engine) => engine.commands).some((command) => command.op === 'placeOrders')).toBe(false);
  });
});

describe('a Telegram bot on a retried start', () => {
  const withBot = { ...init, data: { ...init.data, telegramBotToken: 'bot' } };

  it('starts once the retry brings the account up, not during the wait, and reads the live exchange', async () => {
    const { send } = context();
    void send(withBot);
    await vi.advanceTimersByTimeAsync(121_000);
    expect(telegram.started).toHaveLength(0);
    loads = true;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(exchangeWorkers[ACCOUNT].isRecovering).toBe(false);
    expect(telegram.started).toHaveLength(1);
    expect(telegram.started[0]().isDisposed).toBe(false);
  });

  it('never starts when the retries run out, or without a token', async () => {
    const { send } = context();
    void send(withBot);
    await vi.advanceTimersByTimeAsync(5 * 120_000 + 130_000 + 1_000);
    expect(telegram.started).toHaveLength(0);

    delete exchangeWorkers[ACCOUNT];
    loads = false;
    const other = context();
    void other.send(init);
    await vi.advanceTimersByTimeAsync(121_000);
    loads = true;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(exchangeWorkers[ACCOUNT].isRecovering).toBe(false);
    expect(telegram.started).toHaveLength(0);
  });

  it('still starts once on a first start that succeeds', async () => {
    loads = true;
    const { send } = context();
    void send(withBot);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(telegram.started).toHaveLength(1);
  });
});

describe('a start that fails outright', () => {
  it('does not retry, and answers calls that arrive after it instead of leaving the page on its 121 s timer', async () => {
    setWasmEngineLoader(async () => {
      throw new Error('chunk failed');
    });
    const { messages, send } = context();
    await send(init);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(exchangeWorkers[ACCOUNT]).toBeUndefined();
    await send(read('after-failure'));
    await send(buy('write-after-failure'));
    expect(reply(messages, 'after-failure')[0].data).toMatchObject({ success: false, result: { outcome: 'notSent' } });
    expect(reply(messages, 'write-after-failure')[0].data.result).toMatchObject({ outcome: 'notSent' });
  });

  it('a fresh init replaces the failed worker', async () => {
    let fail = true;
    setWasmEngineLoader(async () => {
      if (fail) throw new Error('chunk failed');
      return { WasmEngine: SlowEngine };
    });
    const { send } = context();
    await send(init);
    await vi.advanceTimersByTimeAsync(1_000);
    fail = false;
    loads = true;
    void send(init);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(exchangeWorkers[ACCOUNT]?.exchange?.isDisposed).toBe(false);
  });
});

describe('a re-init seeded with the page store', () => {
  it('takes no chase or TWAP rows: the algos that ran them died with the old worker', async () => {
    const { messages, send } = context();
    const ghost = {
      id: 'c1',
      symbol: 'BTCUSDT',
      size: 0.001,
      side: OrderSide.Buy,
      maxAttempts: 10,
      attempts: 1,
      isStopped: false,
      stopReason: '',
    };
    void send({
      ...init,
      data: { ...init.data, store: { activeChasers: [ghost], activeTWAPs: [{ id: 't1', symbol: 'BTCUSDT' }] } },
    });
    loads = true;
    await vi.advanceTimersByTimeAsync(2_000);
    expect(exchangeWorkers[ACCOUNT].store.activeChasers).toEqual([]);
    expect(exchangeWorkers[ACCOUNT].store.activeTWAPs).toEqual([]);
    const posted = messages.filter((message) => message.op === 'store');
    expect(posted.some((message) => (message.data?.activeChasers?.length ?? 0) > 0)).toBe(false);
  });
});
