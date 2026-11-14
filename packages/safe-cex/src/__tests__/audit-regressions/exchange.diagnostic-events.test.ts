import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import ts from 'typescript';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { ExchangeEmitter } from '../../exchanges/exchangeEmitter';
import { DefaultStore } from '../../store/store.base';
import { OrderStopTriggerType } from '../../types';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';

// Independent inventory of diagnostic vs live-result events in the worker bridge.
const diagnosticEvents = [
  'engine-event',
  'error',
  'warning',
  'log',
  'exec-log',
  'grafana-info',
  'grafana-error',
  'grafana-pws',
  'grafana-pws-sub',
  'grafana-http-request',
  'grafana-http-response',
  'ws-diagnostic',
];
const liveEvents = ['exec-result', 'chaser:complete'];
const material = 'synthetic-exec-diagnostic-material';
const cleanup: (() => void)[] = [];
afterEach(() => {
  cleanup
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose());
});

function workerBridge() {
  const exchange = new BaseExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  const postMessage = vi.fn();
  const worker = new ExchangeWorker(
    { postMessage } as unknown as Worker,
    'fixture-account',
    'bybit',
    { key: '', secret: '' },
    { placeOrder: { tpTriggerType: OrderStopTriggerType.MarkPrice, slTriggerType: OrderStopTriggerType.MarkPrice } },
    {
      supportedTPSLTriggerTypes: [],
      privateProxyIps: [],
      defaultTPTriggerType: undefined,
      defaultSLTriggerType: undefined,
    },
    'synthetic-session',
  );
  // Exercise real worker subscription/forwarding without starting exchange I/O.
  const setup = worker as unknown as { _exchange: BaseExchange; _setupExchangeEventListeners(): void };
  setup._exchange = exchange;
  setup._setupExchangeEventListeners();
  cleanup.push(() => worker.dispose());
  return { exchange, postMessage };
}

it('classifies every event in the actual worker forwarding inventory', () => {
  const path = resolve(process.cwd(), 'src/webworker/ExchangeWorker.ts');
  const source = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true);
  const inventories: string[][] = [];
  const visit = (node: ts.Node) => {
    if (ts.isVariableDeclaration(node) && node.name.getText(source) === 'logEvents') {
      expect(node.initializer && ts.isArrayLiteralExpression(node.initializer)).toBe(true);
      const array = node.initializer as ts.ArrayLiteralExpression;
      inventories.push(
        array.elements.map((element) => {
          expect(ts.isStringLiteral(element)).toBe(true);
          return (element as ts.StringLiteral).text;
        }),
      );
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  expect(inventories).toHaveLength(1);
  expect(inventories[0].sort()).toEqual([...diagnosticEvents, ...liveEvents].sort());
});

describe.each(diagnosticEvents)('%s diagnostic event', (event) => {
  it('masks sensitive fields before emitter listener delivery', () => {
    const emitter = new ExchangeEmitter('bybit');
    const listener = vi.fn();
    const payload = {
      message: `request failed jwt="${material}"`,
      reqId: 'request-17',
      count: 2,
      status: 400,
      timestamp: 123,
    };
    emitter.on(event, listener);
    emitter.emit(event, payload);
    expect(listener).toHaveBeenCalledOnce();
    const delivered = listener.mock.calls[0][0];
    expect(JSON.stringify(delivered)).not.toContain(material);
    expect(delivered).toMatchObject({ reqId: 'request-17', count: 2, status: 400, timestamp: 123 });
    expect(delivered).not.toBe(payload);
    expect(payload.message).toContain(material);
  });

  it('masks the packet delivered through the real worker event bridge', () => {
    const { exchange, postMessage } = workerBridge();
    const payload = {
      message: `request failed jwt="${material}"`,
      reqId: 'request-17',
      count: 2,
      status: 400,
      timestamp: 123,
    };
    exchange.emitter.emit(event, payload);
    const packets = postMessage.mock.calls.map(([message]) => message).filter((message) => message.op === event);
    expect(packets).toHaveLength(1);
    expect(JSON.stringify(packets[0])).not.toContain(material);
    expect(packets[0]).toMatchObject({
      op: event,
      data: [{ reqId: 'request-17', count: 2, status: 400, timestamp: 123 }],
    });
    expect(payload.message).toContain(material);
  });

  it('preserves every ordinary field and source value', () => {
    const { exchange, postMessage } = workerBridge();
    const payload = {
      message: 'Request completed',
      reqId: 'request-17',
      count: 2,
      status: 200,
      timestamp: 123,
      data: { symbol: 'BTCUSDT' },
    };
    exchange.emitter.emit(event, payload, 'info');
    const packet = postMessage.mock.calls.map(([message]) => message).find((message) => message.op === event);
    expect(packet).toEqual({ op: event, data: [payload, 'info'] });
    expect(payload.data.symbol).toBe('BTCUSDT');
  });
});

it.each(liveEvents)('keeps %s result fields and identity out of diagnostic transformation', (event) => {
  const { exchange, postMessage } = workerBridge();
  const payload = { reqId: 'request-17', success: true, data: { orderIds: ['order-1'], key: material } };
  const listener = vi.fn();
  exchange.on(event, listener);
  exchange.emitter.emit(event, payload);
  expect(listener.mock.calls[0][0]).toBe(payload);
  const packet = postMessage.mock.calls.map(([message]) => message).find((message) => message.op === event);
  expect(packet.data[0]).toBe(payload);
  expect(payload.data.key).toBe(material);
});

it.each([`The exchange refused jwt="${material}"`, 'The exchange refused this key.'])(
  'keeps deactivation status while applying diagnostic policy to refusal text: %s',
  (refusal) => {
    const { exchange, postMessage } = workerBridge();
    exchange.emitter.emit('deactivate', refusal);
    const packet = postMessage.mock.calls
      .map(([message]) => message)
      .find((message) => message.op === 'deactivate-status');
    expect(packet).toMatchObject({ op: 'deactivate-status', data: true });
    expect(packet.refusal).not.toContain(material);
    if (!refusal.includes(material)) expect(packet.refusal).toBe(refusal);
  },
);
