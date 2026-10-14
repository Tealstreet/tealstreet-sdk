import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, describe, expect, it } from 'vitest';

import { isBaseCapabilityFallback } from '../../exchanges/base';
import { exchangeRegistry } from '../../exchanges/registry';
import { DefaultStore } from '../../store/store.base';

/**
 * A base capability stub must be *branded*, or support cannot be detected.
 *
 * `typeof exchange.fetchIncomeHistoryRange === 'function'` is true on every venue,
 * because `BaseExchange` always defines the field. `apps/web/src/components/journal`
 * duck-typed on exactly that, so all 11 exchanges reported "supports income history"
 * and unimplemented ones rendered an empty ledger as though it were real data.
 *
 * The same trap waits for every axis added later, so this tests the mechanism rather
 * than one method.
 */
const BRANDED_STUBS = [
  'fetchIncomeHistory',
  'fetchIncomeHistoryRange',
  'fetchHistoricalOrders',
  'fetchClosedPnl',
] as const;

const options: ExchangeOptions = {
  key: 'capability-branding-key',
  secret: 'capability-branding-secret',
  passphrase: 'capability-branding-passphrase',
  applicationId: 'capability-branding-application',
  apiKeyIndex: '0',
  publicDataOnly: true,
  starkPrivateKey: '0x1',
  paradexAddress: '0x1',
  subkeyAddress: '0x1',
  grvtSignerAddress: '0x0000000000000000000000000000000000000001',
  grvtTradingAccountId: 'capability-branding-account',
};

// A venue implementing NONE of these axes, so its stubs are the untouched base ones.
//
// This was `kraken` until kraken grew a ledger axis (`history/v3/account-log`), at which
// point the test correctly went red — it is asserting that an unimplemented method stays
// detectable, so it needs a venue that is genuinely unimplemented. `binancepm` extends
// `BaseExchange` directly rather than `BinanceExchange`, so it inherits none of binance's
// history axes and is structurally unlikely to grow one incidentally. Confirmed with
// `yarn history-audit --axes`; if this venue ever gains a ledger, move the fixture again
// rather than weakening the assertion.
const NO_LEDGER_VENUE: ExchangeName = 'binancepm' as ExchangeName;

const liveInstances: any[] = [];
const construct = async (name: ExchangeName) => {
  const Exchange = await exchangeRegistry[name]();
  const exchange: any = new Exchange(name, options, new DefaultStore());
  liveInstances.push(exchange);
  return exchange;
};

// dispose() is sync; via afterEach so a failed expectation cannot leak an instance.
afterEach(() => {
  while (liveInstances.length) {
    try {
      liveInstances.pop()?.dispose?.();
    } catch {
      /* teardown is best-effort */
    }
  }
});

describe('base capability stubs are branded', () => {
  it.each(BRANDED_STUBS)('%s on an unimplemented venue is detectable as the fallback', async (method) => {
    const exchange = await construct(NO_LEDGER_VENUE as ExchangeName);

    // The check the journal used to make — true even though nothing is implemented.
    expect(typeof exchange[method]).toBe('function');
    // The check it must make instead.
    expect(isBaseCapabilityFallback(exchange[method], method)).toBe(true);
  });

  it('a real override is not reported as the fallback', () => {
    const overridden = async () => [];
    expect(isBaseCapabilityFallback(overridden, 'fetchIncomeHistory')).toBe(false);
  });

  it('branding is method-specific, so one stub cannot answer for another', async () => {
    const exchange = await construct(NO_LEDGER_VENUE as ExchangeName);
    expect(isBaseCapabilityFallback(exchange.fetchIncomeHistory, 'fetchHistoricalOrders')).toBe(false);
  });

  it.each(['binance', 'bybit', 'bybitinverse', 'okx', 'weex', 'hyperliquid', 'paradex', 'blofin', 'bitunix'])(
    '%s implements the ledger, so it is not reported as the fallback',
    async (name) => {
      const exchange = await construct(name as ExchangeName);
      expect(isBaseCapabilityFallback(exchange.fetchIncomeHistory, 'fetchIncomeHistory')).toBe(false);
      expect(isBaseCapabilityFallback(exchange.fetchIncomeHistoryRange, 'fetchIncomeHistoryRange')).toBe(false);
    },
  );

  it.each(['bybit', 'bybitinverse', 'okx', 'blofin', 'bitunix'])(
    '%s implements closed PnL against the venue',
    async (name) => {
      const exchange = await construct(name as ExchangeName);
      expect(isBaseCapabilityFallback(exchange.fetchClosedPnl, 'fetchClosedPnl')).toBe(false);
    },
  );

  // The order-history axis is the only one that sees orders which never filled — 23% of
  // okx's window, 61% of paradex's. A venue silently falling back to the base stub here
  // does not look broken at the call site; it looks like an account that only ever fills.
  it.each(['bybit', 'binance', 'blofin', 'okx', 'paradex', 'bitunix', 'hyperliquid', 'woo'])(
    '%s implements order history against the venue',
    async (name) => {
      const exchange = await construct(name as ExchangeName);
      expect(isBaseCapabilityFallback(exchange.fetchHistoricalOrders, 'fetchHistoricalOrders')).toBe(false);
    },
  );

  // Spot has no position-based closed PnL, and `supportsClosedPnLEndpoint` is a SOURCE
  // SWITCH for the Closed Positions tab rather than a capability claim — so spot must
  // stay on the stub and the metadata flag must stay false, or its tab renders empty.
  it('bybitspot does NOT implement closed PnL', async () => {
    const exchange = await construct('bybitspot' as ExchangeName);
    expect(isBaseCapabilityFallback(exchange.fetchClosedPnl, 'fetchClosedPnl')).toBe(true);
  });
});
