vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { ExchangeOptions } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { deriveBridgeConstruction, derivePublicMode, deriveSignerSetupPolicy } from '../../exchanges/rust/construction';
import { LocalSignerSetup } from '../../exchanges/rust/localSignerSetup';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const secret = `01${'00'.repeat(39)}`;

describe.each(['lighter', 'lighter-robinhood'] as const)('%s trusted setup cutover', (venue) => {
  it.each([undefined, null, '', ' \t', 'native-key'])(
    'keeps the original key presence %j in the one-use provider capsule',
    (key) => {
      const input = { secret, apiKeyIndex: '1', applicationId: '718895', publicDataOnly: false } as ExchangeOptions;
      if (key !== undefined) Object.assign(input, { key });
      const setup = new LocalSignerSetup(input, deriveSignerSetupPolicy(venue));
      let received: unknown;
      class Engine {
        constructor(_venue: string, _options: string, json?: string) {
          received = JSON.parse(json!);
        }
      }
      setup.construct(Engine as never, venue, '{}');
      expect(received).toEqual({ primary: { ...(key !== undefined ? { key } : {}), secret } });
      expect(derivePublicMode(venue, input)).toEqual({ canSign: true, publicDataOnly: false });
      expect(deriveBridgeConstruction(venue, input).dispatchSigning).toBe(true);
    },
  );

  it('retains only public L1 lookup metadata before stripping credential options', () => {
    const address = `0x${'ab'.repeat(20)}`;
    for (const key of [address, `L1 account: ${address}`]) {
      const exchange = new RustEngineExchange(
        venue,
        {
          key,
          secret,
          apiKeyIndex: '1',
          applicationId: '718895',
          publicDataOnly: false,
        } as ExchangeOptions,
        new DefaultStore(),
      );
      expect(exchange.options.key).toBeUndefined();
      expect(exchange.options.secret).toBeUndefined();
      expect((exchange as unknown as { lighterL1Address: string }).lighterL1Address).toBe(address);
      expect(JSON.stringify(exchange.options)).not.toContain(secret);
    }
    const exchange = new RustEngineExchange(
      venue,
      {
        key: `${address}abcd`,
        secret,
        apiKeyIndex: '1',
        applicationId: '718895',
      } as ExchangeOptions,
      new DefaultStore(),
    );
    expect((exchange as unknown as { lighterL1Address?: string }).lighterL1Address).toBeUndefined();
  });

  it('retains public-only intent without advertising private capability', () => {
    expect(derivePublicMode(venue, { secret, apiKeyIndex: '1', publicDataOnly: true } as ExchangeOptions)).toEqual({
      canSign: true,
      publicDataOnly: true,
    });
    expect(derivePublicMode(venue, { apiKeyIndex: '1' } as ExchangeOptions)).toEqual({
      canSign: false,
      publicDataOnly: true,
    });
  });
});
