import type { ExchangeOptions } from '../../types';

import { BaseExchange } from '../../exchanges/base';
import { BaseWebSocket } from '../../exchanges/base.ws';
import { DefaultStore } from '../../store/store.base';

class WatchdogExchange extends BaseExchange {
  constructor() {
    super('binance', { key: '', secret: '', publicDataOnly: true } as ExchangeOptions, new DefaultStore());
  }

  setPublicSocket(socket: BaseWebSocket<this>) {
    this.publicWebsocket = socket;
  }

  runWatchdogAudit() {
    (this as any).auditSocketHealth();
  }
}

describe('BaseExchange per-socket watchdog timing', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-15T00:00:00.000Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  test('does not bounce a newly opened socket because the exchange is old', () => {
    const exchange = new WatchdogExchange();
    const socket = new BaseWebSocket(exchange);
    socket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    socket.lastConnectedAt = Date.now();
    const reconnect = vi.spyOn(socket, 'reconnect').mockResolvedValue();
    exchange.setPublicSocket(socket);

    exchange.initTime = Date.now() - 10 * 60_000;
    exchange.runWatchdogAudit();

    expect(reconnect).not.toHaveBeenCalled();
    exchange.dispose();
  });

  test('gives a previously initialized reconnect one minute for its first frame', () => {
    const exchange = new WatchdogExchange();
    const socket = new BaseWebSocket(exchange);
    socket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    socket.isInitialized = true;
    socket.lastReceived = 0;
    socket.lastConnectedAt = Date.now();
    const reconnect = vi.spyOn(socket, 'reconnect').mockResolvedValue();
    exchange.setPublicSocket(socket);

    vi.advanceTimersByTime(59_000);
    exchange.runWatchdogAudit();
    expect(reconnect).not.toHaveBeenCalled();

    vi.advanceTimersByTime(2_000);
    exchange.runWatchdogAudit();
    expect(reconnect).toHaveBeenCalledTimes(1);
    exchange.dispose();
  });
});
