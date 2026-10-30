import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';
import type { Exchange } from '../../exchanges/base';
import type { ExchangeName } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';
import { LogSeverity } from '../../types';
import { CallMethodHandler } from '../../webworker/handlers/CallMethodHandler';
import { claimWriteTiming, emitWriteTiming, runWithWriteTiming, WriteTiming } from '../../writeTiming';

afterEach(() => vi.restoreAllMocks());

it('reports each consecutive present stage as one delta and the total', () => {
  const timing = new WriteTiming('placeOrders', { called: 100, received: 103, accepted: 104, sent: 105.04 });
  timing.mark('response', 160);
  timing.mark('reply', 161);
  timing.mark('resolved', 163);
  timing.mark('called', 999); // first stamp wins
  expect(timing.event('ok')).toEqual({
    level: 'info',
    code: 'write.timing',
    message:
      'placeOrders ok 63ms: called>received 3, received>accepted 1, accepted>sent 1, sent>response 55, response>reply 1, reply>resolved 2',
    fields: {
      op: 'placeOrders',
      outcome: 'ok',
      calledToReceivedMs: 3,
      receivedToAcceptedMs: 1,
      acceptedToSentMs: 1,
      sentToResponseMs: 55,
      responseToReplyMs: 1,
      replyToResolvedMs: 2,
      totalMs: 63,
    },
  });
});

it('hands a caller timing only to the write invoked synchronously, and owns one otherwise', () => {
  const caller = new WriteTiming('placeOrder');
  const claimed = runWithWriteTiming(caller, () => claimWriteTiming('placeOrders'));
  expect(claimed).toEqual({ timing: caller, owned: false });
  const own = claimWriteTiming('cancelOrders');
  expect(own.owned).toBe(true);
  expect(own.timing).not.toBe(caller);
  expect(own.timing.marks.called).toBeTypeOf('number');
});

it('takes the existing info engine-event route, never failing the write', () => {
  const events: EngineEvent[] = [];
  const log = vi.fn();
  const timing = new WriteTiming('cancelOrders', { called: 1 });
  emitWriteTiming({ emitter: { emit: (_event, payload) => events.push(payload) }, log }, timing, 'error');
  expect(events).toHaveLength(1);
  expect(events[0]).toMatchObject({ code: 'write.timing', fields: { op: 'cancelOrders', outcome: 'error' } });
  expect(log).toHaveBeenCalledExactlyOnceWith(events[0].message, LogSeverity.Info, events[0]);
  const throwing = {
    emit: () => {
      throw new Error('listener');
    },
  };
  expect(() => emitWriteTiming({ emitter: throwing, log }, new WriteTiming('x'), 'ok')).not.toThrow();
});

it('carries one timing across the worker RPC: caller stamps, worker and engine stamp, caller emits once', async () => {
  const main = new BaseExchange(
    'bybit',
    { key: '', secret: '', publicDataOnly: true, tuning: { consoleLogging: false } },
    new DefaultStore(),
  );
  const workerExchange = {
    name: 'bybit',
    updateSymbolMetadata: vi.fn(),
    // Stands in for RustEngineExchange.placeOrders: claims synchronously, the host stamps engine stages.
    placeOrders: vi.fn((_orders: unknown) => {
      const { timing, owned } = claimWriteTiming('placeOrders');
      expect(owned).toBe(false);
      for (const stage of ['accepted', 'sent', 'response', 'reply'] as const) timing.mark(stage);
      return Promise.resolve(['order-1']);
    }),
  } as unknown as Exchange;
  const handler = new CallMethodHandler(
    'account',
    'bybit' as ExchangeName,
    workerExchange,
    {
      postMessage: (message: { op: string; data: unknown }) => main.emitter.emit(message.op, message.data),
    } as unknown as Worker,
    { placeOrder: {} } as never,
    {} as never,
  );
  main.emitter.on('callMethod', (data: unknown) => handler.handle({ op: 'callMethod', data } as never));
  const events: EngineEvent[] = [];
  main.emitter.on('engine-event', (event: EngineEvent) => events.push(event));

  await expect(main.callMethodInWorker('placeOrders', [] as never)).resolves.toEqual(['order-1']);
  expect(events).toHaveLength(1);
  expect(Object.keys(events[0].fields)).toEqual([
    'op',
    'outcome',
    'calledToReceivedMs',
    'receivedToAcceptedMs',
    'acceptedToSentMs',
    'sentToResponseMs',
    'responseToReplyMs',
    'replyToResolvedMs',
    'totalMs',
  ]);
  expect(events[0].fields).toMatchObject({ op: 'placeOrders', outcome: 'ok' });

  // Reads carry no timing and emit nothing.
  main.emitter.off('callMethod');
  main.emitter.on('callMethod', ({ requestId, ...rest }: { requestId: string }) => {
    expect(rest).not.toHaveProperty('timing');
    main.emitter.emit('callMethod:done', { requestId, success: true, result: [] });
  });
  await main.callMethodInWorker('fetchOrders');
  expect(events).toHaveLength(1);
  main.dispose();
});
