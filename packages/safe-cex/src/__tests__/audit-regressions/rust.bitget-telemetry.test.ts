import { expect, it, vi } from 'vitest';

vi.mock('../../utils/virtualClock', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: vi.fn(async () => undefined) } };
});
const { RustEngineExchange } = await import('../../exchanges/rust/rust.exchange');
class UnstartedRust extends RustEngineExchange {
  override start = async () => {};
}
vi.mock('../../exchanges/registry', () => ({
  exchangeRegistry: {
    get bitget() {
      return async () => UnstartedRust;
    },
  },
}));
const { ExchangeWorker } = await import('../../webworker/ExchangeWorker');

it.each([
  {
    level: 'info',
    code: 'bitget.account.mode',
    message: 'Bitget startup account modes',
    fields: { accountMode: 'uta', posMode: 'hedge', marginMode: 'crossed' },
  },
  {
    level: 'warn',
    code: 'bitget.close.diagnostic',
    message: 'Bitget close order diagnostic',
    fields: {
      symbol: 'BTCUSDT',
      side: 'long',
      requestedSize: '0.6',
      positionTotal: '1',
      reduceOnlyOrderCount: 2,
      reduceOnlyOrderSize: '0.4',
      planOrderCount: 1,
      planOrderSize: '0.2',
      nativeCode: '040757',
      nativeMsg: 'Not enough position is available\n"native"',
    },
  },
])('serializes $code once through the real adapter and worker log bridge for browser logging', async (event) => {
  const messages: any[] = [];
  const worker = new ExchangeWorker(
    { postMessage: (m: unknown) => messages.push(m) } as any,
    'fixture-account',
    'bitget',
    { tuning: { consoleLogging: false } } as any,
    {} as any,
    {} as any,
    'fixture-session',
  );
  await worker.start();
  try {
    const exchange = (worker as any)._exchange as InstanceType<typeof RustEngineExchange>;
    expect(exchange).toBeInstanceOf(RustEngineExchange);
    (exchange as any).receiveEvent(event);
    expect(messages.filter((m) => m.op === 'engine-event')).toEqual([{ op: 'engine-event', data: [event] }]);
    const logs = messages.filter((m) => m.op === 'log' && Array.isArray(m.data) && m.data[2]?.code === event.code);
    expect(logs).toHaveLength(1);
    // The main-thread logging API consumes the string/severity pair, not the optional third argument.
    const [line, severity] = logs[0].data;
    expect(severity).toBe(event.level === 'warn' ? 'warning' : 'info');
    const json = line.slice('[bitget] '.length);
    expect(JSON.parse(json)).toEqual(event);
    expect(line).not.toContain('[object Object]');
    expect(json.split('\n')).toHaveLength(1);
    expect(messages.filter((m) => m.op === 'error')).toHaveLength(0);
  } finally {
    await worker.dispose();
  }
});
