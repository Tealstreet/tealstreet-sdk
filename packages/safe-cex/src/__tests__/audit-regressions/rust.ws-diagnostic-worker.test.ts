import type { WsDiagnosticEvent } from '../../transport/requestsLog';

import { expect, it, vi } from 'vitest';
import { OrderStopTriggerType } from '../../types';

vi.mock('../../utils/virtualClock', async (original) => {
  const actual = await original<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: vi.fn(async () => undefined) } };
});

const { RustEngineExchange } = await import('../../exchanges/rust/rust.exchange');
class UnstartedRust extends RustEngineExchange {
  override start = async () => {};
}
vi.mock('../../exchanges/registry', () => ({
  exchangeRegistry: { hyrotrader: async () => UnstartedRust },
}));
const { ExchangeWorker } = await import('../../webworker/ExchangeWorker');

it('forwards structured Hyro WS diagnostics through the public worker event and removes listeners on disposal', async () => {
  const postMessage = vi.fn();
  const context: Worker = {
    postMessage, onmessage: null, onmessageerror: null, onerror: null,
    terminate: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn(() => true),
  };
  const worker = new ExchangeWorker(
    context,
    'hyro-fixture',
    'hyrotrader',
    { key: '', secret: '', publicDataOnly: true },
    { placeOrder: { tpTriggerType: OrderStopTriggerType.MarkPrice, slTriggerType: OrderStopTriggerType.LastPrice } },
    { supportedTPSLTriggerTypes: [], privateProxyIps: [], defaultTPTriggerType: undefined, defaultSLTriggerType: undefined },
    'session',
  );
  await worker.start();
  const exchange = worker.exchange;
  if (!(exchange instanceof RustEngineExchange)) throw new Error('Expected the worker to construct a Rust exchange');
  const event: WsDiagnosticEvent = {
    venue: 'hyrotrader',
    channel: 'public',
    direction: 'receive',
    message: '{"topic":"tickers.BTCUSDT"}',
  };
  try {
    exchange.emitter.emit('ws-diagnostic', event);
    expect(postMessage.mock.calls.map(([message]) => message).filter(({ op }) => op === 'ws-diagnostic')).toEqual([
      { op: 'ws-diagnostic', data: [event] },
    ]);
  } finally {
    worker.dispose();
  }
  postMessage.mockClear();
  exchange.emitter.emit('ws-diagnostic', event);
  expect(postMessage).not.toHaveBeenCalled();
});
