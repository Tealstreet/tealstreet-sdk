import { describe, expect, it, vi } from 'vitest';

import { HiddenTimerLateness } from '@tealstreet/http-over-websocket/src/connectionDiagnostics';
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

it('passes page visibility through the real worker config handler without accepting free text', async () => {
  vi.useFakeTimers({ now: 10_000 });
  vi.stubGlobal('document', undefined);
  try {
    const { ctx, send } = createCtx();
    start(ctx as any);
    await send({ op: '__workerConfig', data: { connectionVisibility: 'hidden' } });
    const timers = new HiddenTimerLateness();
    timers.observe(0);
    expect(timers.bucket).toBe('5-20s');
    await send({ op: '__workerConfig', data: { connectionVisibility: 'visible' } });
    await send({ op: '__workerConfig', data: { connectionVisibility: 'not-a-visibility' } });
    vi.setSystemTime(90_000);
    timers.observe(0);
    expect(timers.bucket).toBe('5-20s');
  } finally {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  }
});
