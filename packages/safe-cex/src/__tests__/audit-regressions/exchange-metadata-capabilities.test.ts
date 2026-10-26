import type { ExchangeCapabilities } from '../../exchanges/base';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { BaseExchange, CAPABILITY_METHODS } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';
import { ExchangeName, ExchangeOptions } from '../../types';
import { legacyVenueRegistry as exchangeRegistry } from '../reference/legacyVenueRegistry';

vi.hoisted(() => {
  // HyroTrader builds Bybit REST clients during construction, before the test ever starts I/O.
  process.env.NEXT_PUBLIC_HYROTRADER_PROD_REST_URL ??= 'https://hyrotrader-prod.test';
});

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

const expectImplemented = (
  exchange: BaseExchange,
  capability: keyof ExchangeCapabilities,
  exchangeName: ExchangeName,
) => {
  expect(
    exchange.capabilities[capability],
    `${exchangeName}: metadata advertises a capability but ${CAPABILITY_METHODS[capability]} is inert`,
  ).toBe(true);
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
      expectImplemented(await createImplementation(name), 'trading', name);
      expect(exchangeMetadata[name].features.orders, `${name}: stop capability requires order management`).toBe(true);
    }
  });

  it('an advertised options toggle has real option methods behind it', async () => {
    const advertised = (Object.keys(exchangeMetadata) as ExchangeName[]).filter(
      (name) => exchangeMetadata[name].supportsOptionsToggle,
    );

    expect(advertised).toEqual(['bybit']);
    for (const name of advertised) {
      const exchange = await createImplementation(name);
      for (const capability of ['optionChain', 'optionExpiries', 'accountGreeks'] as const) {
        expectImplemented(exchange, capability, name);
      }
    }
  });

  it('advertised public-only support constructs in public mode and overrides startup', async () => {
    const advertised = (Object.keys(exchangeMetadata) as ExchangeName[]).filter(
      (name) => exchangeMetadata[name].supportsPublicOnly,
    );

    for (const name of advertised) {
      expectImplemented(await createImplementation(name, true), 'startup', name);
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
