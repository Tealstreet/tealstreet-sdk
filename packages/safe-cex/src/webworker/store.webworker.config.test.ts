import { describe, expect, it, vi } from 'vitest';

import start, { registerWorkerConfigHandler } from './store.webworker';

const createCtx = () => {
  let listener: ((event: { data: unknown }) => unknown) | undefined;
  const ctx = {
    postMessage: vi.fn(),
    addEventListener: (type: string, fn: (event: { data: unknown }) => unknown) => {
      if (type === 'message') listener = fn;
    },
  };
  return { ctx, send: (data: unknown) => listener?.({ data }) };
};

describe('__workerConfig host handlers', () => {
  it('hands every config to registered host handlers, including keys safe-cex does not own', async () => {
    const handler = vi.fn();
    registerWorkerConfigHandler(handler);
    const { ctx, send } = createCtx();
    start(ctx as any);

    await send({ op: '__workerConfig', data: { wsRelayEnabled: true, wsDiagnosticsEnabled: false } });

    expect(handler).toHaveBeenCalledWith(expect.objectContaining({ wsRelayEnabled: true }));
  });
});
