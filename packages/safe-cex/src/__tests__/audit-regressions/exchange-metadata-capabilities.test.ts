import { afterEach, describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { BaseCapabilityMethod, BaseExchange, isBaseCapabilityFallback, isInertCapability } from '../../exchanges/base';
import { exchangeRegistry } from '../../exchanges/registry';
import { DefaultStore } from '../../store/store.base';
import { ExchangeName, ExchangeOptions } from '../../types';

const liveInstances: BaseExchange[] = [];
/**
 * This test dynamically imports and constructs EVERY registered exchange, so its
 * wall time is dominated by vitest's transform of ~40 exchange module graphs and
 * therefore scales with how loaded the worker pool is, not with anything the test
 * does. Measured on one machine: 4.3 s standalone, 11.3 s under moderate suite
 * load, and it tripped a 15 s cap once the suite grew by three unrelated tests.
 *
 * Raised rather than trimmed because the coverage — every venue constructs
 * without starting I/O — is the point of the file, and a cap that fails on
 * neighbouring tests being added is a flaky gate rather than a strict one.
 */
const REGISTRY_CONSTRUCTION_TIMEOUT_MS = 60_000;

const createImplementation = async (exchangeName: ExchangeName, publicDataOnly = false): Promise<BaseExchange> => {
  const Exchange = await exchangeRegistry[exchangeName]();
  const options: ExchangeOptions = {
    key: 'capability-contract-key',
    secret: 'capability-contract-secret',
    passphrase: 'capability-contract-passphrase',
    applicationId: 'capability-contract-application',
    apiKeyIndex: '0',
    publicDataOnly,
    starkPrivateKey: '0x1',
    paradexAddress: '0x1',
    subkeyAddress: '0x1',
    grvtSignerAddress: '0x0000000000000000000000000000000000000001',
    grvtTradingAccountId: 'capability-contract-account',
  };

  const exchange = new Exchange(exchangeName, options, new DefaultStore());
  liveInstances.push(exchange);
  return exchange;
};

const expectImplemented = (exchange: BaseExchange, method: BaseCapabilityMethod, exchangeName: ExchangeName) => {
  expect(
    isBaseCapabilityFallback(exchange[method], method),
    `${exchangeName}: metadata advertises a capability but ${method} is the inert BaseExchange fallback`,
  ).toBe(false);
};

afterEach(() => {
  liveInstances.splice(0).forEach((exchange) => exchange.dispose());
});

describe('exchange metadata runtime non-fallback invariants', () => {
  it(
    'loads and constructs every registered exchange without starting I/O',
    async () => {
      for (const name of Object.keys(exchangeMetadata) as ExchangeName[]) {
        await expect(
          createImplementation(name, exchangeMetadata[name].supportsPublicOnly),
          `${name}: registry loader or constructor failed`,
        ).resolves.toBeInstanceOf(BaseExchange);
      }
    },
    REGISTRY_CONSTRUCTION_TIMEOUT_MS,
  );

  it('advertised trade history has a non-INERT executions or historical-trades path', async () => {
    // `isInertCapability`, not `isBaseCapabilityFallback`. The branded-fallback check answers
    // "was this never implemented", which is only half the question: an override whose entire
    // body is `return []` is unbranded and passed as a real implementation. coinbase and
    // phemex both ship exactly that for `fetchHistoricalTrades`, so the flag could point at
    // nothing and still satisfy the contract.
    const advertised = (Object.keys(exchangeMetadata) as ExchangeName[]).filter(
      (name) => exchangeMetadata[name].supportsTradesHistory,
    );

    for (const name of advertised) {
      const exchange = await createImplementation(name);
      const hasExecutionHistory = !isInertCapability(exchange.fetchExecutions, 'fetchExecutions');
      const hasHistoricalTrades = !isInertCapability(exchange.fetchHistoricalTrades, 'fetchHistoricalTrades');

      expect(
        hasExecutionHistory || hasHistoricalTrades,
        `${name}: supportsTradesHistory points only at inert history paths (base fallback or empty stub)`,
      ).toBe(true);
    }
  });

  it('advertised executions history is not an empty stub either', async () => {
    const advertised = (Object.keys(exchangeMetadata) as ExchangeName[]).filter(
      (name) => exchangeMetadata[name].supportsExecutionsHistory,
    );

    for (const name of advertised) {
      const exchange = await createImplementation(name);
      expect(
        isInertCapability(exchange.fetchExecutions, 'fetchExecutions'),
        `${name}: supportsExecutionsHistory points at an inert fetchExecutions`,
      ).toBe(false);
    }
  });

  it('detects an empty-stub override as inert, and a real one as live', () => {
    // The mechanism itself, so the two tests above cannot quietly stop discriminating.
    const stub = async () => [];
    const guarded = async (publicOnly: boolean) => {
      if (publicOnly) return [];
      return [1];
    };

    expect(isInertCapability(stub, 'fetchHistoricalTrades')).toBe(true);
    // A real implementation that merely OPENS with a guard clause must not be a stub.
    expect(isInertCapability(guarded, 'fetchHistoricalTrades')).toBe(false);
  });

  it('advertised executions history does not use the inert executions fallback', async () => {
    const advertised = (Object.keys(exchangeMetadata) as ExchangeName[]).filter(
      (name) => exchangeMetadata[name].supportsExecutionsHistory,
    );

    for (const name of advertised) {
      expectImplemented(await createImplementation(name), 'fetchExecutions', name);
    }
  });

  it('advertised order capabilities do not use the inert placement fallback', async () => {
    const advertised = (Object.keys(exchangeMetadata) as ExchangeName[]).filter((name) => {
      const metadata = exchangeMetadata[name];
      return (
        metadata.supportsTrailingStopLoss ||
        metadata.supportsPartialStops ||
        metadata.supportsInfinityStops ||
        metadata.supportedTPSLTriggerTypes.length > 0
      );
    });

    for (const name of advertised) {
      expectImplemented(await createImplementation(name), 'placeOrder', name);
      expect(exchangeMetadata[name].features.orders, `${name}: stop capability requires order management`).toBe(true);
    }
  });

  it('advertised public-only support constructs in public mode and overrides startup', async () => {
    const advertised = (Object.keys(exchangeMetadata) as ExchangeName[]).filter(
      (name) => exchangeMetadata[name].supportsPublicOnly,
    );

    for (const name of advertised) {
      expectImplemented(await createImplementation(name, true), 'start', name);
    }
  });

  it('publishes internally consistent TP/SL trigger selections', () => {
    for (const name of Object.keys(exchangeMetadata) as ExchangeName[]) {
      const metadata = exchangeMetadata[name];
      const supported = metadata.supportedTPSLTriggerTypes;

      expect(new Set(supported).size, `${name}: supported TP/SL trigger types must be unique`).toBe(supported.length);

      if (metadata.defaultTPTriggerType !== undefined) {
        expect(supported, `${name}: default TP trigger must be advertised`).toContain(metadata.defaultTPTriggerType);
      }
      if (metadata.defaultSLTriggerType !== undefined) {
        expect(supported, `${name}: default SL trigger must be advertised`).toContain(metadata.defaultSLTriggerType);
      }
    }
  });
});
