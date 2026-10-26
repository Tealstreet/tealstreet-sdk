import type { PublicExchangeHandle } from '../publicExchange/PublicExchangeFactory';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { NativeFrameSession } from './NativeFrameSession';
import { ServerSessionFactory, type NativeAccountRequest } from './ServerSessionFactory';

vi.mock('@tealstreet/safe-cex/src/exchanges/rust/rust.exchange', () => ({
  isRustVenue: (name: string) => name === 'binance',
}));
vi.mock('./NativeFrameSession', () => ({ NativeFrameSession: { connect: vi.fn() } }));

const sink = { onFrame: vi.fn(), onNotice: vi.fn(), onClose: vi.fn() };
const target = { url: 'ws://localhost:9001/accounts/account-1', token: 'test-token' };
const request = { engine: 'rust', exchange: 'binance', accountId: 'account-1', sink } as const;

describe('ServerSessionFactory', () => {
  beforeEach(() => vi.clearAllMocks());

  it('delegates the TS handle unchanged, leaving readiness, refresh and ownership in the public factory', async () => {
    const handle = { forceRefreshMarkets: vi.fn(), dispose: vi.fn() } as unknown as PublicExchangeHandle;
    const get = vi.fn().mockResolvedValue(handle);
    const resolveNativeAccount = vi.fn();
    const factory = new ServerSessionFactory({ publicExchangeFactory: { get }, resolveNativeAccount });
    expect(await factory.open({ engine: 'typescript', exchange: 'binance' })).toBe(handle);
    expect(get).toHaveBeenCalledExactlyOnceWith('binance');
    expect(resolveNativeAccount).not.toHaveBeenCalled();
    expect(NativeFrameSession.connect).not.toHaveBeenCalled();
    expect(handle.forceRefreshMarkets).not.toHaveBeenCalled();
    expect(handle.dispose).not.toHaveBeenCalled();
  });

  it('resolves the authorized account AND venue without constructing a TS exchange', async () => {
    const get = vi.fn();
    const resolveNativeAccount = vi.fn().mockResolvedValue(target);
    const session = { ready: true };
    vi.mocked(NativeFrameSession.connect).mockResolvedValue(session as NativeFrameSession);
    const factory = new ServerSessionFactory({
      publicExchangeFactory: { get },
      resolveNativeAccount,
      nativeOptions: { maxQueuedMessages: 8 },
    });
    expect(await factory.open(request)).toBe(session);
    expect(resolveNativeAccount).toHaveBeenCalledExactlyOnceWith({
      exchange: 'binance',
      accountId: 'account-1',
      signal: expect.any(AbortSignal),
    });
    expect(NativeFrameSession.connect).toHaveBeenCalledExactlyOnceWith({
      ...target,
      sink,
      signal: undefined,
      maxQueuedMessages: 8,
    });
    expect(get).not.toHaveBeenCalled();
  });

  it('fails closed on native connection failure instead of falling back to TS', async () => {
    const get = vi.fn();
    vi.mocked(NativeFrameSession.connect).mockRejectedValueOnce(new Error('Native connection failed'));
    const factory = new ServerSessionFactory({
      publicExchangeFactory: { get },
      resolveNativeAccount: async () => target,
    });
    await expect(factory.open(request)).rejects.toThrow('Native connection failed');
    expect(get).not.toHaveBeenCalled();
    expect(NativeFrameSession.connect).toHaveBeenCalledTimes(1);
  });

  it('refuses unsupported venues and unsafe account identifiers before provisioning', async () => {
    const resolveNativeAccount = vi.fn();
    const factory = new ServerSessionFactory({ resolveNativeAccount });
    await expect(factory.open({ ...request, exchange: 'bitunix' })).rejects.toThrow('does not support');
    await expect(factory.open({ ...request, exchange: 'constructor' as 'binance' })).rejects.toThrow(
      'does not support',
    );
    await expect(factory.open({ ...request, exchange: '__proto__' as 'binance' })).rejects.toThrow('does not support');
    await expect(factory.open({ ...request, accountId: '../other' })).rejects.toThrow('Invalid native account');
    expect(resolveNativeAccount).not.toHaveBeenCalled();
    expect(NativeFrameSession.connect).not.toHaveBeenCalled();
  });

  it('redacts resolution errors and refuses missing accounts without a socket', async () => {
    const factory = new ServerSessionFactory({
      resolveNativeAccount: vi
        .fn()
        .mockRejectedValueOnce(new Error('secret resolver credentials'))
        .mockResolvedValueOnce(null),
    });
    await expect(factory.open(request)).rejects.toThrow('Native account resolution failed');
    await expect(factory.open(request)).rejects.toThrow('Native account is unavailable');
    expect(NativeFrameSession.connect).not.toHaveBeenCalled();
  });

  it('cancels account resolution and never connects a late target', async () => {
    const controller = new AbortController();
    let finish!: (value: typeof target) => void;
    const resolving = new Promise<typeof target>((resolve) => {
      finish = resolve;
    });
    const resolveNativeAccount = vi.fn(() => resolving);
    const factory = new ServerSessionFactory({ resolveNativeAccount });
    const opening = factory.open({ ...request, signal: controller.signal });
    await vi.waitFor(() => expect(resolveNativeAccount).toHaveBeenCalledOnce());
    controller.abort('secret abort reason');
    await expect(opening).rejects.toThrow('Native account resolution failed');
    finish(target);
    await Promise.resolve();
    expect(NativeFrameSession.connect).not.toHaveBeenCalled();
  });

  it('bounds resolver time and rejects pre-aborted requests before resolving', async () => {
    const resolveNativeAccount = vi.fn((_request: NativeAccountRequest) => new Promise<null>(() => {}));
    const factory = new ServerSessionFactory({ resolveNativeAccount, resolveTimeoutMs: 1 });
    await expect(factory.open(request)).rejects.toThrow('Native account resolution failed');
    expect(resolveNativeAccount).toHaveBeenCalledOnce();
    expect(resolveNativeAccount.mock.calls[0][0].signal?.aborted).toBe(true);
    const controller = new AbortController();
    controller.abort();
    await expect(factory.open({ ...request, signal: controller.signal })).rejects.toThrow(
      'Native account resolution failed',
    );
    expect(resolveNativeAccount).toHaveBeenCalledOnce();
    expect(NativeFrameSession.connect).not.toHaveBeenCalled();
  });

  it('requires explicit TS configuration', async () => {
    const factory = new ServerSessionFactory({ resolveNativeAccount: async () => target });
    await expect(factory.open({ engine: 'typescript', exchange: 'binance' })).rejects.toThrow('not configured');
  });
});
