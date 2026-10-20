import { afterEach, expect, it, vi } from 'vitest';

import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { RustEngineExchange, setRustEngineTransport } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { virtualClock } from '@tealstreet/safe-cex/src/utils/virtualClock';

import { readConfig } from './config';
import { createServer } from './server';
import { rustConsumerFixture } from './testing/rustConsumerFixture';

vi.mock('@tealstreet/safe-cex/src/utils/virtualClock', async (original) => {
  const actual = await original<typeof import('@tealstreet/safe-cex/src/utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: async () => {} } };
});

let instance: Awaited<ReturnType<typeof createServer>> | undefined;
let client: Client | undefined;
afterEach(async () => {
  await client?.close();
  await instance?.server.close();
  instance?.exchange.dispose();
  virtualClock.stop();
});

it('serves real Rust account and market reads through the MCP protocol', async () => {
  const fixture = rustConsumerFixture();
  setRustEngineTransport(() => fixture.transport);
  instance = await createServer(
    readConfig({
      EXCHANGE: 'coinbase',
      EXCHANGE_ENGINE: 'rust',
      EXCHANGE_API_KEY: fixture.options.key,
      EXCHANGE_API_SECRET: fixture.options.secret,
    }),
    'test',
  );
  expect(instance.exchange).toBeInstanceOf(RustEngineExchange);
  const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair();
  await instance.server.connect(serverTransport);
  client = new Client({ name: 'rust-consumer-proof', version: '1' });
  await client.connect(clientTransport);
  const names = (await client.listTools()).tools.map(({ name }) => name);
  expect(names).toContain('get_balance');
  const read = async (name: string) => {
    const result = await client!.callTool({ name, arguments: {} });
    expect(result.isError).not.toBe(true);
    const content = result.content as Array<{ type: string; text: string }>;
    return JSON.parse(content[0].text);
  };
  expect(await read('get_markets')).toContain('BTC-CDE');
  expect(await read('get_balance')).toMatchObject({ total: 1000, free: 800 });
  expect(await read('get_positions')).toEqual(
    expect.arrayContaining([expect.objectContaining({ symbol: 'BTC-CDE', contracts: 0.1 })]),
  );
  expect((await read('get_open_orders')).map((order: { id: string }) => order.id)).toEqual(['11', '12', '13', '21']);
  expect(fixture.failures).toEqual([]);
  expect(fixture.requests.some((request) => request.endsWith('/portfolios/replay-portfolio'))).toBe(true);
  // The MCP server's disablePublicWebsockets policy must reach the native host.
  expect(fixture.sockets).toEqual(['wss://advanced-trade-ws-user.coinbase.com']);
}, 15000);
