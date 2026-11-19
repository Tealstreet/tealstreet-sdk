import type { StoreWorkerPayload } from './StoreWorkerPayload';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { nativeStoreFrame } from '../__tests__/cutover/nativeFixture';
import { RustEngineExchange } from '../exchanges/rust/rust.exchange';
import { FrameStore } from '../store/frameStore';
import { OrderStopTriggerType } from '../types';
import { ExchangeWorker } from './ExchangeWorker';

const workers: ExchangeWorker[] = [];
afterEach(() => workers.splice(0).forEach((worker) => worker.dispose()));

function createWorker() {
  const messages: StoreWorkerPayload[] = [];
  const postMessage = vi.fn((message: StoreWorkerPayload) => {
    messages.push(message);
  });
  const worker = new ExchangeWorker(
    { postMessage },
    'account',
    'bybit',
    { key: '', secret: '' },
    { placeOrder: { tpTriggerType: OrderStopTriggerType.LastPrice, slTriggerType: OrderStopTriggerType.LastPrice } },
    {
      supportedTPSLTriggerTypes: [],
      privateProxyIps: [],
      defaultTPTriggerType: undefined,
      defaultSLTriggerType: undefined,
    },
    'session',
  );
  workers.push(worker);
  return { worker, messages, postMessage };
}

describe('native worker state transport', () => {
  it('sends the exact native snapshot bytes without a store side channel', () => {
    const { worker, messages, postMessage } = createWorker();
    const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, worker.store);
    const bytes = nativeStoreFrame({
      activeChasers: [],
      activeTWAPs: [],
      displayedMargin: { longExposure: null, shortExposure: 0, totalNotional: null },
    });
    vi.spyOn(exchange, 'snapshotFrame').mockReturnValue(bytes);
    Object.defineProperties(worker, { _exchange: { value: exchange }, _forwardsFrames: { value: true } });
    worker.sendSnapshot();
    expect(postMessage).toHaveBeenCalledExactlyOnceWith({ op: 'frame', data: bytes, snapshot: true }, [bytes]);
    expect(messages.map((message) => message.op)).toEqual(['frame']);
    const receiver = new FrameStore();
    receiver.ingest(new Frame(bytes), { snapshot: true });
    expect(receiver.displayedMargin).toEqual({ longExposure: null, shortExposure: 0, totalNotional: null });
  });

  it('publishes no payload when native generated state changes', () => {
    const { worker, messages } = createWorker();
    worker.store.ingest(new Frame(nativeStoreFrame({ balance: { used: 1, free: 2, total: 3, upnl: 0 } })));
    expect(worker.store.balance.total).toBe(3);
    expect(messages).toEqual([]);
  });

  it('does not send snapshots after its native exchange is disposed', () => {
    const { worker, messages } = createWorker();
    const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, worker.store);
    Object.defineProperties(worker, { _exchange: { value: exchange }, _forwardsFrames: { value: true } });
    exchange.dispose();
    worker.sendSnapshot();
    expect(messages).toEqual([]);
  });
});
