import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { describe, expect, it, vi } from 'vitest';

import { createEngineTransport } from './engine';
import { prepareRecordedHttp } from '../__tests__/support/managedHttp';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';

vi.mock('../index', () => ({ createExchange: vi.fn() }));

const INFO = 'https://api.hyperliquid.xyz/info';
const KEYED = { key: '0x14791697260e4c9a71f18484c9f997b308e59325', secret: 's', transport: tealstreetTransport };

/** A relay socket that never opens, as when the account's relay is unreachable. */
class SilentSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  readonly readyState = 0;
  binaryType = 'arraybuffer';
  onopen: unknown = null;
  onclose: unknown = null;
  onmessage: unknown = null;
  onerror: unknown = null;
  addEventListener() {}
  removeEventListener() {}
  send() {}
  close() {}
}

const raw = () => ({
  now: () => 0,
  WebSocket: SilentSocket as unknown as typeof WebSocket,
  prepareHttp: prepareRecordedHttp,
  request: vi.fn<Transport['request']>(async () => ({ status: 200, body: new TextEncoder().encode('[]') })),
  connect: vi.fn<Transport['connect']>(() => ({ send: vi.fn(), close: vi.fn() })),
  setTimeout: vi.fn<Transport['setTimeout']>(),
  clearTimeout: vi.fn<Transport['clearTimeout']>(),
});

describe('Hyperliquid public reads on a keyed account', () => {
  it.each(['hyperliquid', 'hyperliquidspot'] as const)('%s market data goes direct, never through the tunnel', async (exchange) => {
    const native = raw();
    const transport = createEngineTransport(exchange, KEYED, native);
    const packet = {
      request: 1,
      method: 'POST',
      url: INFO,
      headers: { 'Content-Type': 'application/json' },
      body: '{"type":"metaAndAssetCtxs"}',
      timeoutMs: 5_000,
    };
    const prepared = await transport.prepareHttp!({ ...packet, intent: 'public' } as never);
    const response = await prepared.bind(packet as never).dispatch();
    expect(response).toMatchObject({ status: 200 });
    expect(native.request).toHaveBeenCalledTimes(1);
    expect(native.request.mock.calls[0][0].url).toBe(INFO);
    expect(native.connect).not.toHaveBeenCalled();
  });
});

describe('a relay that does not connect', () => {
  it('names the relay once for the account, and private reads are not sent', async () => {
    const native = raw();
    const reports: string[] = [];
    const transport = createEngineTransport('hyperliquid', KEYED, native, undefined, (message) => reports.push(message));
    const packet = {
      request: 2,
      method: 'POST',
      url: INFO,
      headers: { 'Content-Type': 'application/json' },
      body: '{"type":"clearinghouseState","user":"0x14791697260e4c9a71f18484c9f997b308e59325"}',
      timeoutMs: 300,
    };
    const read = () =>
      Promise.resolve(transport.prepareHttp!({ ...packet, intent: 'primary' } as never)).then((prepared) =>
        prepared.bind(packet as never).dispatch(),
      );
    await expect(read()).rejects.toBeDefined();
    await expect(read()).rejects.toBeDefined();
    expect(native.request).not.toHaveBeenCalled();
    expect(reports).toEqual([expect.stringContaining('The connection to the Tealstreet relay failed')]);
  });
});
