import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BaseWebSocket } from '../exchanges/base.ws';

// Coming back online used to rebuild the whole account, because a plain
// `reconnect()` loses to the state an offline spell leaves a socket in.

class TestSocket extends BaseWebSocket<any> {
  connectAndSubscribe = vi.fn();
  onOpen = () => undefined;
  onMessage = () => undefined;
}

const socket = () => {
  const parent: any = {
    name: 'test',
    options: { key: 'k' },
    isDisposed: false,
    log: vi.fn(),
    emitter: { emit: vi.fn() },
    syncSocketHealthToStore: vi.fn(),
  };
  return new TestSocket(parent);
};

/** Each reconnect fails the way an offline link does, until the failure-streak pause is on. */
const failUntilPaused = async (s: TestSocket) => {
  for (let i = 0; i < 8; i += 1) {
    (s as any).onClose({ code: 1006 });
    await vi.advanceTimersByTimeAsync(i < 7 ? 40_000 : 0);
  }
  expect(s.isReconnectPauseActive).toBe(true);
  s.connectAndSubscribe.mockClear();
};

describe('BaseWebSocket.recoverAfterOffline', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('connects within seconds instead of waiting out the failure-streak pause', async () => {
    const paused = socket();
    await failUntilPaused(paused);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(paused.connectAndSubscribe).not.toHaveBeenCalled();

    expect(paused.recoverAfterOffline()).toBe(true);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(paused.isReconnectPauseActive).toBe(false);
    expect(paused.connectAndSubscribe).toHaveBeenCalledTimes(1);

    // The paused timer was replaced, not left to fire a second connect.
    await vi.advanceTimersByTimeAsync(130_000);
    expect(paused.connectAndSubscribe).toHaveBeenCalledTimes(1);
  });

  it('revives a socket stuck past the 100-close cap', async () => {
    const stuck = socket();
    stuck.onCloseCount = 101;
    (stuck as any).onClose({ code: 1006 });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(stuck.connectAndSubscribe).not.toHaveBeenCalled();

    stuck.recoverAfterOffline();
    await vi.advanceTimersByTimeAsync(5_000);

    expect(stuck.connectAndSubscribe).toHaveBeenCalledTimes(1);
  });

  it('closes a socket that still looks open, and its close event reconnects it', async () => {
    const halfOpen = socket();
    const close = vi.fn(() => (halfOpen as any).onClose({ code: 1006 }));
    (halfOpen as any).ws = { close, removeEventListener: vi.fn(), readyState: 1 };

    halfOpen.recoverAfterOffline();
    await vi.advanceTimersByTimeAsync(5_000);

    expect(close).toHaveBeenCalledTimes(1);
    expect(halfOpen.connectAndSubscribe).toHaveBeenCalledTimes(1);
  });

  // An async listen-key or token fetch has no socket yet; a second connect would leak one.
  it('leaves a connect that is already in flight alone', async () => {
    const connecting = socket();

    connecting.recoverAfterOffline();
    await vi.advanceTimersByTimeAsync(10_000);

    expect(connecting.connectAndSubscribe).not.toHaveBeenCalled();
  });

  // Online, offline and online again: the second recovery must not kill the first's connect.
  it('leaves a socket that is still connecting alone', () => {
    const connecting = socket();
    const close = vi.fn();
    (connecting as any).ws = { close, removeEventListener: vi.fn(), readyState: 0 };

    connecting.recoverAfterOffline();

    expect(close).not.toHaveBeenCalled();
  });

  it('reports a socket that runs its own close loop as unable to recover', () => {
    class OwnLoop extends TestSocket {
      protected readonly recoversAfterOffline = false;
    }
    const ownLoop = new OwnLoop(socket().parent);

    expect(ownLoop.recoverAfterOffline()).toBe(false);
  });

  it('does nothing for a disposed exchange or a force-closed socket', async () => {
    const disposed = socket();
    disposed.parent.isDisposed = true;
    disposed.recoverAfterOffline();
    const closed = socket();
    closed.forceClose();
    closed.recoverAfterOffline();
    await vi.advanceTimersByTimeAsync(10_000);

    expect(disposed.connectAndSubscribe).not.toHaveBeenCalled();
    expect(closed.connectAndSubscribe).not.toHaveBeenCalled();
  });
});
