vi.mock('../../../packages/safe-cex/src/exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../../../packages/safe-cex/src/__tests__/reference/retainedRustPolicy');
  return retainedRustPolicy(
    await importOriginal<typeof import('../../../packages/safe-cex/src/exchanges/rust/betaVenues')>(),
    true,
  );
});

import { afterEach, expect, it, vi } from 'vitest';

import { formatAccountMonitorOutput } from '@tealstreet/cli-core/src/commands/trading/MonitorCommand';

import {
  fixtureAccountTransport,
  rustConsumerFixture,
} from '../../../packages/exchange-mcp/src/testing/rustConsumerFixture';
import {
  RustEngineExchange,
  setRustEngineTransport,
} from '../../../packages/safe-cex/src/exchanges/rust/rust.exchange';
import { virtualClock } from '../../../packages/safe-cex/src/utils/virtualClock';
import { setDefaultTransport } from '../../../packages/safe-cex/src/transport/registry';
import { directTransport } from '../../../packages/safe-cex/src/transport/direct';
import { createExchangeFromConfig, startExchange, stopExchange } from './exchange-factory';

vi.mock('../../../packages/safe-cex/src/utils/virtualClock', async (original) => {
  const actual = await original<typeof import('../../../packages/safe-cex/src/utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: async () => {} } };
});

let exchange: RustEngineExchange | undefined;
afterEach(async () => {
  if (exchange) await stopExchange(exchange);
  virtualClock.stop();
  setDefaultTransport(directTransport);
});

it('starts a saved account through the CLI factory and reads native account state', async () => {
  const fixture = rustConsumerFixture();
  setRustEngineTransport(() => fixture.transport);
  setDefaultTransport(fixtureAccountTransport);
  const saved = JSON.parse(
    JSON.stringify({
      name: 'Saved Coinbase',
      exchange: 'coinbase',
      apiKey: fixture.options.key,
      apiSecret: fixture.options.secret,
      tealstreetProxy: false,
      tealstreetTunnel: false,
    }),
  );
  exchange = (await createExchangeFromConfig(saved)) as RustEngineExchange;
  expect(exchange).toBeInstanceOf(RustEngineExchange);
  const startup = startExchange(exchange);
  void startup.catch(() => {});
  await vi.waitFor(
    () =>
      expect(
        exchange!.store.loaded.balance,
        JSON.stringify({ requests: fixture.requests, failures: fixture.failures, loaded: exchange!.store.loaded }),
      ).toBe(true),
    { timeout: 5000 },
  );
  await startup;
  expect(exchange.store.markets.map((market) => market.symbol)).toContain('BTC-CDE');
  expect(exchange.store.balance.total).toBe(1000);
  expect(exchange.store.balance.free).toBe(800);
  expect(exchange.store.loaded.balance).toBe(true);
  expect(await exchange.fetchPositions()).toEqual(
    expect.arrayContaining([expect.objectContaining({ symbol: 'BTC-CDE', contracts: 0.1 })]),
  );
  expect((await exchange.fetchOrders()).map((order) => order.id)).toEqual(['11', '12', '13', '21']);
  expect(formatAccountMonitorOutput(exchange)).toContain('BTC-CDE');
  expect(formatAccountMonitorOutput(exchange)).toContain('1,000');
  expect(fixture.requests.some((request) => request.endsWith('/portfolios/replay-portfolio'))).toBe(true);
  expect(fixture.sockets).toContain('wss://advanced-trade-ws-user.coinbase.com');
  expect(fixture.failures).toEqual([]);
}, 15000);
