vi.mock('@tealstreet/safe-cex/src/exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('@tealstreet/safe-cex/src/__tests__/reference/retainedRustPolicy');
  return retainedRustPolicy(
    await importOriginal<typeof import('@tealstreet/safe-cex/src/exchanges/rust/betaVenues')>(),
    true,
  );
});

import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

import { afterEach, expect, it, vi } from 'vitest';

import { createExchangeFromAccount } from '@tealstreet/safe-cex/src/createExchangeFromAccount';
import { RustEngineExchange, setRustEngineTransport } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { virtualClock } from '@tealstreet/safe-cex/src/utils/virtualClock';
import { directTransport } from '@tealstreet/safe-cex/src/transport/direct';
import { setDefaultTransport } from '@tealstreet/safe-cex/src/transport/registry';

import { fixtureAccountTransport, rustConsumerFixture } from '../../exchange-mcp/src/testing/rustConsumerFixture';
import { setReactNativeRustEngineLoader } from './index';

vi.mock('@tealstreet/safe-cex/src/utils/virtualClock', async (original) => {
  const actual = await original<typeof import('@tealstreet/safe-cex/src/utils/virtualClock')>();
  return { ...actual, virtualClock: { ...actual.virtualClock, start: async () => {} } };
});

const accounts: RustEngineExchange[] = [];
afterEach(() => {
  for (const account of accounts.splice(0)) account.dispose();
  virtualClock.stop();
  setDefaultTransport(directTransport);
});

it('loads two saved accounts through the mobile host loader with one native module initialization', async () => {
  const module = await import('@tealstreet/safe-cex-wasm/web');
  const bytes = readFileSync(new URL('../../../sdk-rs/hosts/wasm/pkg/web/safe_cex_wasm_bg.wasm', import.meta.url));
  const loader = vi.fn(async () => {
    await module.default({ module_or_path: bytes });
    return module;
  });
  await setReactNativeRustEngineLoader(loader);
  setDefaultTransport(fixtureAccountTransport);
  const fixtures = [rustConsumerFixture(), rustConsumerFixture()];
  let index = 0;
  setRustEngineTransport(() => fixtures[index++].transport);
  for (const fixture of fixtures) {
    const exchange = await createExchangeFromAccount({
      name: 'Native saved account',
      exchange: 'coinbase',
      apiKey: fixture.options.key,
      apiSecret: fixture.options.secret,
    });
    expect(exchange).toBeInstanceOf(RustEngineExchange);
    accounts.push(exchange as RustEngineExchange);
  }
  await Promise.all(accounts.map((account) => account.start()));
  expect(loader).toHaveBeenCalledTimes(1);
  for (const account of accounts) {
    expect(account.store.markets.map((market) => market.symbol)).toContain('BTC-CDE');
    expect(account.store.balance.total).toBe(1000);
    expect(account.store.loaded.balance).toBe(true);
  }
  expect(fixtures.flatMap((fixture) => fixture.failures)).toEqual([]);
}, 15000);

it('resolves the actual native registry and starts its Rust class through the mobile host loader', () => {
  // Metro's static require resolves TypeScript. Vite's Node require does not;
  // use a TS-capable runtime for the unchanged native registry, without mocks.
  const output = execFileSync('bun', ['scripts/native-registry-probe.ts'], {
    cwd: new URL('..', import.meta.url),
    timeout: 30000,
    encoding: 'utf8',
  });
  expect(output).toContain('mobile-native-registry: Binance aliases loaded a native catalog');
}, 35000);
