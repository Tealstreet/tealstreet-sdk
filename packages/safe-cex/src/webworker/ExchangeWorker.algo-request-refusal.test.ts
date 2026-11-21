import { describe, expect, it, vi } from 'vitest';

import { OrderStopTriggerType } from '../types';
import { ExchangeWorker } from './ExchangeWorker';

function stoppedWorker() {
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
  worker.dispose();
  return { worker, postMessage };
}

describe('LOCAL algorithm requests that cannot be dispatched', () => {
  it.each(['chaser:start', 'chaser:amend', 'twap:start', 'twap:amend'] as const)('refuses %s visibly', (op) => {
    const { worker, postMessage } = stoppedWorker();
    worker.handlePayload({ op, data: { id: 'job', update: { maxAttempts: 5 } } } as never);
    const refusals = postMessage.mock.calls
      .map(([message]) => message)
      .filter((message) => message.op === 'error' && message.data[1]?.sentryData?.extra?.payloadOp === op);
    expect(refusals).toHaveLength(1);
  });
});
