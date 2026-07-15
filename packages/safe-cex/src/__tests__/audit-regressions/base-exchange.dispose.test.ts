import { BaseExchange } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';

describe('BaseExchange disposal cleanup', () => {
  test('unsubscribes its store bridge and removes retained lifecycle listeners', () => {
    const store = new DefaultStore();
    const exchange = new BaseExchange('binance', { key: '', secret: '', publicDataOnly: true }, store);
    const logListener = vi.fn();
    const reconnectListener = vi.fn();
    const signerListener = vi.fn();
    const grafanaListener = vi.fn();
    const httpTelemetryListener = vi.fn();

    exchange.on('log', logListener);
    exchange.on('ws-reconnected', reconnectListener);
    exchange.on('remoteSignerStatus', signerListener);
    exchange.on('grafana-error', grafanaListener);
    exchange.on('grafana-http-request', httpTelemetryListener);

    expect((store as any).listeners.size).toBe(1);

    exchange.dispose();
    store.update({ latency: 42 });
    exchange.emitter.emit('log', 'after-dispose');
    exchange.emitter.emit('ws-reconnected', '');
    exchange.emitter.emit('remoteSignerStatus', 'connected');
    exchange.emitter.emit('grafana-error', 'after-dispose');
    exchange.emitter.emit('grafana-http-request', { url: '/after-dispose' });

    expect((store as any).listeners.size).toBe(0);
    expect(logListener).not.toHaveBeenCalled();
    expect(reconnectListener).not.toHaveBeenCalled();
    expect(signerListener).not.toHaveBeenCalled();
    expect(grafanaListener).not.toHaveBeenCalled();
    expect(httpTelemetryListener).not.toHaveBeenCalled();
  });
});
