import { BinanceExchange } from '../reference/binance/binance.exchange';
import { BinancePrivateWebsocket } from '../reference/binance/binance.ws-private';
import { BinancePMExchange } from '../reference/binance/binancepm.exchange';
import { BinancePMPrivateWebsocket } from '../reference/binance/binancepm.ws-private';
import { loop } from '../../utils/loop';

vi.mock('../../utils/loop', () => ({ loop: vi.fn() }));

/**
 * In hosted mode a Binance listenKey request and the startup order fetch are
 * signed by the browser over a link that can drop. A failed listenKey used to
 * reject un-awaited and leave the private stream unbuilt with nothing retrying; a
 * failed keepalive ended the refresh chain; and a failed startup order fetch
 * returned the (empty) store and marked it loaded, showing "no open orders"
 * until some order event happened to arrive. Binance PM copies all three.
 */

const SOCKETS = [
  { label: 'binance', Ctor: BinancePrivateWebsocket, xhr: 'xhr' },
  { label: 'binancepm', Ctor: BinancePMPrivateWebsocket, xhr: 'papiXhr' },
] as const;

const makeParent = (xhrName: string, post: ReturnType<typeof vi.fn>, put = vi.fn()) => ({
  isDisposed: false,
  name: 'binance',
  options: { key: 'k', secret: 's', testnet: false },
  emitter: { emit: vi.fn() },
  store: { update: vi.fn(), loaded: {} },
  log: vi.fn(),
  emitAndLogError: vi.fn(),
  [xhrName]: { post, put },
});

describe('binance user data stream recovery', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  describe.each(SOCKETS)('$label', ({ Ctor, xhr }) => {
    it('retries a failed listenKey request with backoff, then connects', async () => {
      const post = vi
        .fn()
        .mockRejectedValueOnce(new Error('Remote signer unavailable — WebSocket disconnected (gone <5s)'))
        .mockResolvedValue({ data: { listenKey: 'lk-1' } });
      const socket = new Ctor(makeParent(xhr, post) as any) as any;
      const created: string[] = [];
      vi.spyOn(socket, 'createWebsocket').mockImplementation((url: unknown) => {
        created.push(String(url));
        return { addEventListener: vi.fn() };
      });

      await socket.connectAndSubscribe();
      expect(created).toEqual([]);
      expect(String(socket.parent.emitAndLogError.mock.calls[0][0])).toContain('Binance user data stream unavailable');

      await vi.advanceTimersByTimeAsync(5_000);
      expect(post).toHaveBeenCalledTimes(2);
      expect(created).toHaveLength(1);
      expect(created[0]).toContain('lk-1');
    });

    it('builds one socket when connects overlap, and a forceClose cancels a pending retry', async () => {
      let release!: () => void;
      const post = vi.fn(
        () =>
          new Promise((resolve) => {
            release = () => resolve({ data: { listenKey: 'lk' } });
          }),
      );
      const socket = new Ctor(makeParent(xhr, post) as any) as any;
      const createWebsocket = vi.spyOn(socket, 'createWebsocket').mockReturnValue({ addEventListener: vi.fn() });

      const first = socket.connectAndSubscribe();
      const second = socket.connectAndSubscribe();
      release();
      await Promise.all([first, second]);
      expect(post).toHaveBeenCalledTimes(1);
      expect(createWebsocket).toHaveBeenCalledTimes(1);

      const failing = new Ctor(makeParent(xhr, vi.fn().mockRejectedValue(new Error('signer down'))) as any) as any;
      const failingCreate = vi.spyOn(failing, 'createWebsocket').mockReturnValue({ addEventListener: vi.fn() });
      await failing.connectAndSubscribe();
      failing.forceClose?.();
      await vi.advanceTimersByTimeAsync(60_000);
      expect(failing.parent[xhr].post).toHaveBeenCalledTimes(1);
      expect(failingCreate).not.toHaveBeenCalled();
    });

    it('does not build a socket when forceClose lands while the listenKey is in flight', async () => {
      let release!: () => void;
      const post = vi.fn(
        () =>
          new Promise((resolve) => {
            release = () => resolve({ data: { listenKey: 'lk' } });
          }),
      );
      const socket = new Ctor(makeParent(xhr, post) as any) as any;
      const createWebsocket = vi.spyOn(socket, 'createWebsocket').mockReturnValue({ addEventListener: vi.fn() });

      const connecting = socket.connectAndSubscribe();
      socket.forceClose();
      release();
      await connecting;
      expect(createWebsocket).not.toHaveBeenCalled();
    });

    it('toasts once per failure streak, then resyncs orders when the retried stream opens', async () => {
      const post = vi
        .fn()
        .mockRejectedValueOnce(new Error('signer down'))
        .mockRejectedValueOnce(new Error('signer down'))
        .mockResolvedValue({ data: { listenKey: 'lk' } });
      const socket = new Ctor(makeParent(xhr, post) as any) as any;
      const listeners: Record<string, () => void> = {};
      vi.spyOn(socket, 'createWebsocket').mockReturnValue({
        addEventListener: (event: string, fn: () => void) => (listeners[event] = fn),
      });
      vi.spyOn(socket, 'ping').mockImplementation(() => undefined);
      const fetchOrders = vi.spyOn(socket, 'fetchThrottledOrders').mockResolvedValue(undefined);

      await socket.connectAndSubscribe();
      await vi.advanceTimersByTimeAsync(5_000);
      await vi.advanceTimersByTimeAsync(10_000);
      expect(post).toHaveBeenCalledTimes(3);
      expect(socket.parent.emitAndLogError).toHaveBeenCalledTimes(1);

      listeners.open();
      await vi.advanceTimersByTimeAsync(10_000);
      expect(fetchOrders).toHaveBeenCalledTimes(1);
    });

    it('resyncs orders when a reconnected stream opens, but not on the first open', async () => {
      const post = vi.fn().mockResolvedValue({ data: { listenKey: 'lk' } });
      const socket = new Ctor(makeParent(xhr, post) as any) as any;
      vi.spyOn(socket, 'createWebsocket').mockReturnValue({ addEventListener: vi.fn() });
      vi.spyOn(socket, 'ping').mockImplementation(() => undefined);
      const fetchOrders = vi.spyOn(socket, 'fetchThrottledOrders').mockResolvedValue(undefined);

      socket.onOpen();
      await vi.advanceTimersByTimeAsync(10_000);
      expect(fetchOrders).not.toHaveBeenCalled();

      socket.onOpen();
      await vi.advanceTimersByTimeAsync(10_000);
      expect(fetchOrders).toHaveBeenCalledTimes(1);
    });

    it('closes a live socket before building its replacement', async () => {
      const post = vi.fn().mockResolvedValue({ data: { listenKey: 'lk' } });
      const socket = new Ctor(makeParent(xhr, post) as any) as any;
      const first = {
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        readyState: WebSocket.OPEN,
        close: vi.fn(),
      };
      vi.spyOn(socket, 'createWebsocket').mockReturnValueOnce(first).mockReturnValue({ addEventListener: vi.fn() });

      await socket.connectAndSubscribe();
      await socket.connectAndSubscribe();
      expect(first.removeEventListener).toHaveBeenCalledWith('close', socket.onClose);
      expect(first.close).toHaveBeenCalledTimes(1);
    });

    it('keeps one keepalive chain, and a failed keepalive does not end it', async () => {
      const post = vi.fn().mockResolvedValue({ data: { listenKey: 'lk' } });
      const put = vi.fn().mockRejectedValueOnce(new Error('timed out')).mockResolvedValue({});
      const socket = new Ctor(makeParent(xhr, post, put) as any) as any;
      vi.spyOn(socket, 'createWebsocket').mockReturnValue({ addEventListener: vi.fn() });

      // A reconnect re-runs connectAndSubscribe; it must not start a second chain.
      await socket.connectAndSubscribe();
      await socket.connectAndSubscribe();
      await vi.advanceTimersByTimeAsync(30 * 60 * 1000);
      expect(put).toHaveBeenCalledTimes(1);

      await vi.advanceTimersByTimeAsync(60_000);
      expect(put).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(30 * 60 * 1000);
      expect(put).toHaveBeenCalledTimes(3);
    });
  });
});

describe('binance startup orders after a failed fetch', () => {
  const EXCHANGES = [
    {
      label: 'binance',
      make: (store: any) => new BinanceExchange('binance', { key: 'k', secret: 's' }, store) as any,
      failFetch: (exchange: any) => vi.spyOn(exchange.xhr, 'get').mockRejectedValue(new Error('signer down')),
    },
    {
      label: 'binancepm',
      make: (store: any) => new BinancePMExchange('binancepm', { key: 'k', secret: 's' }, store) as any,
      failFetch: (exchange: any) =>
        vi.spyOn(exchange, 'fetchRegularOrders').mockRejectedValue(new Error('signer down')),
    },
  ] as const;

  it.each(EXCHANGES)('$label: stays unloaded, then the retry loads it', async ({ make, failFetch }) => {
    const store = { loaded: {} as Record<string, boolean>, orders: [], subscribe: vi.fn(), update: vi.fn() };
    const exchange = make(store);
    exchange.emitAndLogError = vi.fn();
    exchange.log = vi.fn();
    const failing = failFetch(exchange);

    await exchange.fetchOrders();
    expect(exchange.ordersSnapshotStale).toBe(true);

    failing.mockRestore();
    vi.spyOn(exchange, 'fetchOrders').mockImplementation(async () => {
      exchange.ordersSnapshotStale = false;
      return [{ id: 'o1' }];
    });
    await exchange.retryOrdersSnapshot();
    expect(store.update).toHaveBeenCalledWith(
      expect.objectContaining({ orders: [{ id: 'o1' }], loaded: expect.objectContaining({ orders: true }) }),
    );
    expect(loop).not.toHaveBeenCalled();
  });
});
