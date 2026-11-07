import { afterEach, describe, expect, it, vi } from 'vitest';

import { BybitExchange } from './__tests__/reference/bybit/bybit.exchange';
import { DefaultStore } from './store/store.base';
import { reportEvent, reportException, setTelemetrySink } from './telemetry';
import { directTransport } from './transport/direct';

describe('telemetry sink', () => {
  afterEach(() => setTelemetrySink({}));

  it('sends nothing anywhere when the host registered no sink', () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    expect(() => reportEvent('anything', { a: 1 })).not.toThrow();
    expect(() => reportException(new Error('x'), {})).not.toThrow();
    expect(fetchSpy).not.toHaveBeenCalled();
    fetchSpy.mockRestore();
  });

  it('delivers events and exceptions to the registered sink', () => {
    const event = vi.fn();
    const captureException = vi.fn();
    setTelemetrySink({ event, captureException });

    reportEvent('hyperliquid.builder-fee-approved', { wallet: '0xabc', testnet: false });
    const error = new Error('boom');
    reportException(error, { tags: { exchange: 'bybit' } });

    expect(event).toHaveBeenCalledWith('hyperliquid.builder-fee-approved', { wallet: '0xabc', testnet: false });
    expect(captureException).toHaveBeenCalledWith(error, { tags: { exchange: 'bybit' } });
  });

  it('reports a main-thread exchange error through the sink, tagged with the exchange', () => {
    const captureException = vi.fn();
    setTelemetrySink({ captureException });
    const exchange = new BybitExchange(
      'bybit',
      { key: '', secret: '', publicDataOnly: true, transport: directTransport },
      new DefaultStore(),
    );
    try {
      exchange.emitAndLogError(new Error('venue said no'));
      expect(captureException).toHaveBeenCalledTimes(1);
      const [, context] = captureException.mock.calls[0];
      expect(context.tags).toMatchObject({ exchange: 'bybit', module: 'Exchange' });
    } finally {
      exchange.dispose();
    }
  });
});
