import type { Exchange } from '../../exchanges/base';

import { expect, it, vi } from 'vitest';

import { EngineDeadError } from '@tealstreet/safe-cex-wasm/host';
import { CallMethodHandler } from './CallMethodHandler';

it('keeps native interrupted-write identity in the correlated failure reply', async () => {
  const error = new EngineDeadError(new WebAssembly.RuntimeError('fixture'), {
    op: 'placeOrders',
    clientOrderId: 'client-1',
    clientOrderIds: ['client-1', 'client-2'],
  });
  const messages: unknown[] = [];
  const handler = new CallMethodHandler(
    'account',
    'binance',
    {
      setLeverage: vi.fn().mockRejectedValue(error),
    } as unknown as Exchange,
    { postMessage: (message: unknown) => messages.push(message) } as unknown as Worker,
    {} as never,
    {} as never,
  );
  await handler.handleCallMethod('request-1', 'setLeverage', ['BTCUSDT', 2]);
  expect(messages).toEqual([
    {
      op: 'callMethod:done',
      data: {
        requestId: 'request-1',
        success: false,
        result: expect.objectContaining({
          code: 'ENGINE_DEAD',
          kind: 'internal',
          outcome: 'unknown',
          clientOrderId: 'client-1',
          clientOrderIds: ['client-1', 'client-2'],
        }),
      },
    },
  ]);
  handler.dispose();
});
