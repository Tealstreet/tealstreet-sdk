// @vitest-environment node
import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { MessageChannel } from 'node:worker_threads';

import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustPublicFeedProvider } from '../../exchanges/rust/publicFeed';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { realVenue } from './realVenue';

const accounts: RustEngineExchange[] = [];
const inputs: ReturnType<typeof realVenue>[] = [];
let provider: RustPublicFeedProvider | undefined;
afterEach(() => {
  for (const account of accounts.splice(0).reverse()) account.dispose();
  provider?.dispose();
  for (const input of inputs.splice(0)) input.dispose();
});

it('releases one native Aster candle subscription after its listeners leave', async () => {
  setWasmEngineLoader(loadNodeEngine);
  const input = realVenue('asterdex');
  inputs.push(input);
  const packets: Array<{ method?: string; params?: string[] }> = [];
  const transport: Transport = {
    ...input.transport,
    connect(url, handlers) {
      const socket = input.transport.connect(url, handlers);
      return {
        close: () => socket.close(),
        send(text) {
          try {
            packets.push(JSON.parse(text));
          } catch {
            /* Heartbeats can be plain text. */
          }
          socket.send(text);
        },
      };
    },
  };
  setRustEngineTransport(() => transport);
  const source = new RustEngineExchange('asterdex', input.options, new DefaultStore());
  accounts.push(source);
  const sourceReady = source.start();
  void sourceReady.catch(() => {});
  expect(await input.replayPublicFrames()).toBeGreaterThan(0);
  await sourceReady;
  provider = new RustPublicFeedProvider(source);
  const channel = new MessageChannel();
  provider.attach(channel.port1 as unknown as MessagePort);
  const receiverInput = realVenue('asterdex');
  inputs.push(receiverInput);
  setRustEngineTransport(() => receiverInput.transport);
  const receiver = new RustEngineExchange(
    'asterdex',
    {
      ...receiverInput.options,
      publicFeed: { mode: 'shared', port: channel.port2 as unknown as MessagePort },
    },
    new DefaultStore(),
  );
  accounts.push(receiver);
  await receiver.start();
  expect(receiverInput.connectedSocketUrls).toEqual([]);
  const symbol = source.store.markets.find((market) => market.symbol === 'BTCUSDT')!.symbol;
  const topic = `${symbol.toLowerCase()}@kline_1h`;
  const matching = (method: string) =>
    packets.filter((packet) => packet.method === method && packet.params?.includes(topic));
  const first = receiver.listenOHLCV({ symbol, interval: '1h' }, () => {});
  const last = receiver.listenOHLCV({ symbol, interval: '1h' }, () => {});
  await vi.waitFor(() => expect(matching('SUBSCRIBE')).toHaveLength(1), { timeout: 5000 });
  first();
  await new Promise<void>((resolve) => setImmediate(resolve));
  expect(matching('UNSUBSCRIBE')).toHaveLength(0);
  last();
  await vi.waitFor(() => expect(matching('UNSUBSCRIBE')).toHaveLength(1), { timeout: 5000 });
  expect(input.errors).toEqual([]);
  expect(receiverInput.errors).toEqual([]);
}, 15000);
