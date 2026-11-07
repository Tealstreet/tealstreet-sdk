import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BingxPrivateWebsocket } from './reference/bingx/bingx.ws-private';
import { BitruePrivateWebsocket } from './reference/bitrue/bitrue.ws-private';

// The browser often reports "online" before the link works. A listen-key fetch
// that fails then used to leave the private socket dead until a full rebuild.

const parent = (post: ReturnType<typeof vi.fn>) =>
  ({
    name: 'test',
    options: { key: 'k', testnet: false },
    isDisposed: false,
    log: vi.fn(),
    emitter: { emit: vi.fn() },
    emitAndLogError: vi.fn(),
    syncSocketHealthToStore: vi.fn(),
    xhr: { post, put: vi.fn() },
  }) as any;

const linkDown = () => vi.fn().mockRejectedValue(Object.assign(new Error('Network Error'), { code: 'ERR_NETWORK' }));

describe('private socket listen-key retry', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('BingX retries a failed listen-key fetch instead of throwing and staying dead', async () => {
    const post = linkDown();
    const socket = new BingxPrivateWebsocket(parent(post));

    await expect(socket.connectAndSubscribe()).resolves.toBeUndefined();
    expect(post).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(31_000);
    expect(post.mock.calls.length).toBeGreaterThan(1);
    expect((socket as any).parent.emitAndLogError).toHaveBeenCalledTimes(1);
  });

  it('Bitrue keeps retrying with backoff after its quick attempts are spent', async () => {
    const post = linkDown();
    const socket = new BitruePrivateWebsocket(parent(post));

    void socket.connectAndSubscribe();
    await vi.advanceTimersByTimeAsync(2_000);
    const quickAttempts = post.mock.calls.length;
    expect(quickAttempts).toBe(4);

    await vi.advanceTimersByTimeAsync(30_000);
    expect(post.mock.calls.length).toBeGreaterThan(quickAttempts);
    // One toast for the outage, not one per retry.
    expect((socket as any).parent.emitAndLogError).toHaveBeenCalledTimes(1);
  });

  // An outage usually begins with the socket dropping; its first failed fetch still says so.
  it('toasts the first failed fetch of an outage that began with a dropped socket', async () => {
    const post = linkDown();
    const socket = new BingxPrivateWebsocket(parent(post));
    (socket as any).onClose({ code: 1006 });

    await socket.connectAndSubscribe();

    expect((socket as any).parent.emitAndLogError).toHaveBeenCalledTimes(1);
  });
});
