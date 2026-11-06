vi.mock('@tealstreet/safe-cex/src/exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('@tealstreet/safe-cex/src/__tests__/reference/retainedRustPolicy');
  return retainedRustPolicy(
    await importOriginal<typeof import('@tealstreet/safe-cex/src/exchanges/rust/betaVenues')>(),
    true,
  );
});

import { afterEach, expect, it, vi } from 'vitest';

import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import {
  RustEngineExchange,
  setRustEngineTransport,
  setWasmEngineLoader,
} from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { loadNodeEngine } from '@tealstreet/safe-cex/src/exchanges/rust/loadNodeEngine';
import { virtualClock } from '@tealstreet/safe-cex/src/utils/virtualClock';

import { readConfig } from './config';
import { createServer } from './server';
import { rustBinancePublicFixture, rustBreakoutFixture, rustConsumerFixture } from './testing/rustConsumerFixture';

it('rejects the HMAC proxy signer before starting a default Rust account', async () => {
  const fixture = rustConsumerFixture();
  setRustEngineTransport(() => fixture.transport);
  await expect(
    createServer(
      readConfig({
        EXCHANGE: 'coinbase',
        EXCHANGE_API_KEY: fixture.options.key,
        EXCHANGE_SIGNER_HEADER: 'X-Test-Signature',
        EXCHANGE_SIGNER_SIGNATURE: 'placeholder',
      }),
      'test',
    ),
  ).rejects.toThrow('EXCHANGE_SIGNER_HEADER is unsupported');
  expect(fixture.requests).toEqual([]);
  expect(fixture.sockets).toEqual([]);
});

it.each(['binance', 'binancepm', 'binanceinverse', 'binancespot'] as const)(
  'admits %s HMAC proxy signers through declared Rust provider setup before startup', async (name) => {
    const fixture = rustBinancePublicFixture();
    setRustEngineTransport(() => fixture.transport);
    instance = await createServer(readConfig({ EXCHANGE: name, EXCHANGE_ENGINE: 'rust',
      EXCHANGE_API_KEY: 'proxy-public', EXCHANGE_SIGNER_HEADER: 'X-Test-Signature', EXCHANGE_SIGNER_SIGNATURE: 'placeholder' }), 'test');
    expect(instance.exchange).toBeInstanceOf(RustEngineExchange);
    expect(instance.exchange.isPublicDataOnly).toBe(false);
    expect(instance.exchange.options).not.toHaveProperty('payloadSigner');
    expect(instance.exchange.options).not.toHaveProperty('secret');
    expect(fixture.requests).toEqual([]);
    expect(fixture.sockets).toEqual([]);
  },
);

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
  setWasmEngineLoader(loadNodeEngine);
});

it('serves real Rust account and market reads through the MCP protocol', async () => {
  const fixture = rustConsumerFixture();
  const loader = vi
    .fn()
    .mockRejectedValueOnce(new Error('temporary engine load failure'))
    .mockImplementation(loadNodeEngine);
  setWasmEngineLoader(loader);
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
  const first = instance.exchange;
  const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair();
  await instance.server.connect(serverTransport);
  client = new Client({ name: 'rust-consumer-proof', version: '1' });
  await client.connect(clientTransport);
  const names = (await client.listTools()).tools.map(({ name }) => name);
  expect(names).toContain('get_balance');
  const failed = await client.callTool({ name: 'get_markets', arguments: {} });
  expect(failed.isError).toBe(true);
  expect(JSON.stringify(failed.content)).toContain('temporary engine load failure');
  const read = async (name: string) => {
    const result = await client!.callTool({ name, arguments: {} });
    expect(result.isError).not.toBe(true);
    const content = result.content as Array<{ type: string; text: string }>;
    return JSON.parse(content[0].text);
  };
  const [markets, balance] = await Promise.all([read('get_markets'), read('get_balance')]);
  expect(markets).toContain('BTC-CDE');
  expect(balance).toMatchObject({ total: 1000, free: 800 });
  expect(instance.exchange).not.toBe(first);
  expect(loader).toHaveBeenCalledTimes(2);
  expect(await read('get_positions')).toEqual(
    expect.arrayContaining([expect.objectContaining({ symbol: 'BTC-CDE', contracts: 0.1 })]),
  );
  expect((await read('get_open_orders')).map((order: { id: string }) => order.id)).toEqual(['11', '12', '13', '21']);
  expect(fixture.failures).toEqual([]);
  expect(fixture.requests.some((request) => request.endsWith('/portfolios/replay-portfolio'))).toBe(true);
  expect(fixture.sockets).toEqual([
    'wss://advanced-trade-ws.coinbase.com',
    'wss://advanced-trade-ws-user.coinbase.com',
  ]);
}, 15000);

it('keeps native Binance ticker tools fresh through recorded public socket input', async () => {
  const fixture = rustBinancePublicFixture();
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => fixture.transport);
  instance = await createServer(readConfig({ EXCHANGE: 'binance', EXCHANGE_ENGINE: 'rust' }), 'test');
  const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair();
  await instance.server.connect(serverTransport);
  client = new Client({ name: 'native-ticker-consumer', version: '1' });
  await client.connect(clientTransport);
  const tickers = async (): Promise<Array<{ symbol: string; last: number; bid: number; ask: number }>> => {
    const result = await client!.callTool({ name: 'get_tickers', arguments: {} });
    expect(result.isError).not.toBe(true);
    return JSON.parse((result.content as Array<{ text: string }>)[0].text);
  };
  const initial = new Map((await tickers()).map((ticker) => [ticker.symbol, ticker]));
  expect(instance.exchange).toBeInstanceOf(RustEngineExchange);
  expect(initial.size).toBeGreaterThan(0);
  expect(fixture.sockets.length).toBeGreaterThan(0);
  expect(await fixture.replayPublicFrames()).toBeGreaterThan(0);
  await vi.waitFor(
    async () => {
      const changed = (await tickers()).some((ticker) => {
        const before = initial.get(ticker.symbol);
        return before && (ticker.last !== before.last || ticker.bid !== before.bid || ticker.ask !== before.ask);
      });
      expect(changed).toBe(true);
    },
    { timeout: 5000 },
  );
  expect(fixture.failures).toEqual([]);
}, 15000);

it('retains native REST polling for BreakoutProp MCP ticker reads', async () => {
  const fixture = rustBreakoutFixture();
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => fixture.transport);
  instance = await createServer(
    readConfig({
      EXCHANGE: 'breakoutprop',
      EXCHANGE_ENGINE: 'rust',
      EXCHANGE_API_KEY: 'replay-key',
      EXCHANGE_API_SECRET: 'replay-secret',
    }),
    'test',
  );
  const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair();
  await instance.server.connect(serverTransport);
  client = new Client({ name: 'native-poll-consumer', version: '1' });
  await client.connect(clientTransport);
  const readTickers = async (): Promise<Array<{ symbol: string; bid: number }>> => {
    const result = await client!.callTool({ name: 'get_tickers', arguments: {} });
    expect(result.isError).not.toBe(true);
    return JSON.parse((result.content as Array<{ text: string }>)[0].text);
  };
  const initial = new Map((await readTickers()).map((ticker) => [ticker.symbol, ticker.bid]));
  const initialRequests = fixture.requests.length;
  expect(initial.size).toBe(5);
  expect(fixture.sockets).toEqual([]);
  await vi.waitFor(
    async () => {
      expect((await readTickers()).some((ticker) => initial.get(ticker.symbol) !== ticker.bid)).toBe(true);
    },
    { timeout: 6000 },
  );
  expect(fixture.requests.length).toBeGreaterThan(initialRequests);
  expect(fixture.failures).toEqual([]);
}, 15000);

it('selects Rust by default without disabling native polling or public sockets', async () => {
  const disablePublic = vi.spyOn(RustEngineExchange.prototype, 'disablePublicWebsockets');
  instance = await createServer(readConfig({ EXCHANGE: 'bybit' }), 'test');
  expect(instance.exchange).toBeInstanceOf(RustEngineExchange);
  expect(instance.exchange.options.extra?.tickInterval).not.toBe(Number.POSITIVE_INFINITY);
  expect(disablePublic).not.toHaveBeenCalled();
  disablePublic.mockRestore();
});
