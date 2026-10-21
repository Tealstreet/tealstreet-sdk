import type { Exchange } from '../../exchanges/base';
import type { StoreWorkerPayload } from '../../webworker/StoreWorkerPayload';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import { BaseExchange } from '../../exchanges/base';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { CallMethodHandler } from '../../webworker/handlers/CallMethodHandler';

const exchanges: BaseExchange[] = [];
const handlers: CallMethodHandler[] = [];
afterEach(() => {
  handlers.splice(0).forEach((handler) => handler.dispose());
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.useRealTimers();
});

const marked = <T>(rows: T[]): T[] => Object.defineProperty(rows, 'incomplete', { value: true });
const transport = (
  clone: (value: StoreWorkerPayload) => StoreWorkerPayload,
  method: 'fetchHistoricalFills' | 'fetchClosedPnl',
) => {
  const main = new RustEngineExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  exchanges.push(main);
  const calls: StoreWorkerPayload[] = [];
  const reply = (message: StoreWorkerPayload) => {
    calls.push(clone(message));
    main.emitter.emit('callMethod:done', calls.at(-1)!.data);
  };
  const read = vi.fn(async ({ symbol }: { symbol: string }) =>
    symbol === 'partial' ? marked([{ id: symbol }]) : [{ id: symbol }],
  );
  const worker = { name: 'bybit', [method]: read } as unknown as Exchange;
  const handler = new CallMethodHandler(
    'account',
    'bybit',
    worker,
    { postMessage: reply } as unknown as Worker,
    {} as never,
    {} as never,
  );
  handlers.push(handler);
  main.emitter.on(
    'callMethod',
    ({ requestId, method: call, args }: { requestId: string; method: never; args: unknown[] }) => {
      void handler.handleCallMethod(requestId, call, args);
    },
  );
  return { main, calls, read };
};

for (const [name, clone] of [
  ['structured clone', (value: StoreWorkerPayload) => structuredClone(value)],
  ['hosted JSON', (value: StoreWorkerPayload) => JSON.parse(JSON.stringify(value)) as StoreWorkerPayload],
] as const) {
  describe(name, () => {
    it.each(['fetchHistoricalFills', 'fetchClosedPnl'] as const)(
      'preserves %s completeness only for its matching request',
      async (method) => {
        const { main, calls } = transport(clone, method);
        const partial = main.callMethodInWorker(method, { symbol: 'partial' });
        const complete = main.callMethodInWorker(method, { symbol: 'unflagged' });
        const [a, b] = await Promise.all([partial, complete]);
        expect(a).toEqual([{ id: 'partial' }]);
        expect(b).toEqual([{ id: 'unflagged' }]);
        expect(a).toHaveProperty('incomplete', true);
        expect(Object.getOwnPropertyDescriptor(a, 'incomplete')?.enumerable).toBe(false);
        expect(b).not.toHaveProperty('incomplete');
        const flagged = calls.find((message) => message.data.result[0].id === 'partial')!;
        expect(flagged.data).toMatchObject({
          requestId: expect.any(String),
          success: true,
          completeness: { incomplete: true },
        });
        expect(calls.find((message) => message.data.result[0].id === 'unflagged')!.data).not.toHaveProperty(
          'completeness',
        );
      },
    );
  });
}

it('ignores a late flagged reply after timeout rather than marking a later request', async () => {
  vi.useFakeTimers();
  const main = new BaseExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  exchanges.push(main);
  const ids: string[] = [];
  main.emitter.on('callMethod', ({ requestId }: { requestId: string }) => ids.push(requestId));
  const timedOut = main.callMethodInWorker('fetchHistoricalFills', {}).catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(5000);
  expect(await timedOut).toMatchObject({ message: 'Timeout callMethodInWorker' });
  const current = main.callMethodInWorker('fetchHistoricalFills', {});
  main.emitter.emit('callMethod:done', {
    requestId: ids[0],
    success: true,
    result: [],
    completeness: { incomplete: true },
  });
  main.emitter.emit('callMethod:done', { requestId: ids[1], success: true, result: [] });
  expect(await current).not.toHaveProperty('incomplete');
});

it('keeps a correlated terminal failure rejected even with an incomplete record', async () => {
  const main = new BaseExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  exchanges.push(main);
  main.emitter.on('callMethod', ({ requestId }: { requestId: string }) => {
    main.emitter.emit('callMethod:done', {
      requestId,
      success: false,
      result: { message: 'history failed' },
      completeness: { incomplete: true },
    });
  });
  await expect(main.callMethodInWorker('fetchIncomeHistory')).rejects.toThrow('history failed');
});

it('restores a delayed Rust forwarded result before its observer without flagging a faster peer', async () => {
  const { main, read } = transport((value) => JSON.parse(JSON.stringify(value)), 'fetchHistoricalFills');
  let release!: (rows: Array<{ id: string }>) => void;
  read.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  const observed = vi.spyOn(main, 'afterMethodCalledInWorker');
  const partial = main.fetchHistoricalFills({ symbol: 'partial' });
  const peer = await main.fetchHistoricalFills({ symbol: 'unflagged' });
  expect(peer).not.toHaveProperty('incomplete');
  release(marked([{ id: 'partial' }]));
  const result = await partial;
  expect(result).toHaveProperty('incomplete', true);
  const args = observed.mock.calls.find(([rows]) => rows === (result as unknown));
  expect(args?.[0]).toBe(result);
  expect(args?.[0]).toHaveProperty('incomplete', true);
});

it('carries an actual command-correlated host event through the worker JSON result boundary', async () => {
  const host = new EngineHost(
    {
      command: (json: string) => {
        const { command, symbol } = JSON.parse(json);
        return JSON.stringify([
          ...(symbol === 'partial'
            ? [{ op: 'event', level: 'warn', code: 'history.incomplete', message: 'capped', fields: { command } }]
            : []),
          { op: 'done', command, result: JSON.stringify([{ id: symbol }]) },
        ]);
      },
    } as never,
    { now: () => 1 } as never,
  );
  try {
    const { main, read } = transport((value) => JSON.parse(JSON.stringify(value)), 'fetchHistoricalFills');
    read.mockImplementation(
      ({ symbol }) => host.command({ op: 'fetchFills', symbol }) as Promise<Array<{ id: string }>>,
    );
    const [partial, peer] = await Promise.all([
      main.fetchHistoricalFills({ symbol: 'partial' }),
      main.fetchHistoricalFills({ symbol: 'unflagged' }),
    ]);
    expect(partial).toEqual([{ id: 'partial' }]);
    expect(partial).toHaveProperty('incomplete', true);
    expect(peer).toEqual([{ id: 'unflagged' }]);
    expect(peer).not.toHaveProperty('incomplete');
  } finally {
    host.dispose();
  }
});
