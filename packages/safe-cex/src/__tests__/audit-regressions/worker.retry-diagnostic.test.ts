import { afterEach, expect, it, vi } from 'vitest';

import { OrderStopTriggerType } from '../../types';

vi.mock('../../utils/virtualClock', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: vi.fn(async () => undefined) } };
});
const { RustEngineExchange } = await import('../../exchanges/rust/rust.exchange');
const fixture = vi.hoisted(() => ({ message: '' }));
class WaitingRust extends RustEngineExchange {
  override start = async () => {
    // Real native-event ingress: this updates lastReadFailure before startup times out.
    (this as any).receiveEvent({ code: 'request.failed', level: 'debug', message: fixture.message, fields: {} });
    await new Promise<void>(() => {});
  };
}
vi.mock('../../exchanges/registry', () => ({
  exchangeRegistry: {
    get bybit() {
      return async () => WaitingRust;
    },
  },
}));
const { ExchangeWorker } = await import('../../webworker/ExchangeWorker');
const material = 'synthetic-retry-diagnostic-material';
afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
});

it.each([`gateway failed jwt="${material}"`, 'gateway busy (request-17)'])(
  'keeps retry warnings diagnostic-only after a real startup timeout: %s',
  async (message) => {
    vi.useFakeTimers();
    fixture.message = message;
    const postMessage = vi.fn();
    const worker = new ExchangeWorker(
      { postMessage } as unknown as Worker,
      'fixture-account',
      'bybit',
      { key: '', secret: '' },
      { placeOrder: { tpTriggerType: OrderStopTriggerType.MarkPrice, slTriggerType: OrderStopTriggerType.MarkPrice } },
      {
        supportedTPSLTriggerTypes: [],
        privateProxyIps: [],
        defaultTPTriggerType: undefined,
        defaultSLTriggerType: undefined,
      },
      'synthetic-session',
    );
    try {
      const pending = worker.start();
      await vi.advanceTimersByTimeAsync(120_000);
      expect(await pending).toBe(false);
      expect(worker.retryTimedOutStart()).toBe(true);
      const warnings = postMessage.mock.calls.map(([packet]) => packet).filter((packet) => packet.op === 'warning');
      expect(warnings).toHaveLength(1);
      expect(JSON.stringify(warnings)).not.toContain(material);
      expect(warnings[0].data[0]).toContain('retrying in 10s.');
      expect(warnings[0].data[1]).toEqual({ forceToast: true });
      expect((worker as unknown as { _startAnswer?: string })._startAnswer).not.toContain(material);
      if (!message.includes(material)) expect(warnings[0].data[0]).toContain(message);
      expect(fixture.message).toBe(message);
    } finally {
      worker.dispose();
    }
  },
);
