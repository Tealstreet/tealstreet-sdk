import { describe, expect, it } from 'vitest';
import vectors from '../../exchanges/rust/hyperliquid-signer-setup.golden.json';
import { mapHyperliquidSignerSetup } from '../../exchanges/rust/hyperliquidSignerSetup';
import { deriveSignerSetupPolicy } from '../../exchanges/rust/construction';
import { LocalSignerSetup } from '../../exchanges/rust/localSignerSetup';
import type { ExchangeOptions } from '../../types';

describe('Hyperliquid trusted ScalarOnly setup capsule', () => {
  for (const venue of ['hyperliquid', 'hyperliquidspot'] as const) {
    for (const row of vectors) {
      it(`${venue}: ${row.name} reaches the one native decoder unchanged`, () => {
        const options = {
          key: '0x1111111111111111111111111111111111111111',
          applicationId: '0x2222222222222222222222222222222222222222',
          vault: '0x3333333333333333333333333333333333333333',
          secret: row.scalar,
        } as ExchangeOptions;
        const mapped = mapHyperliquidSignerSetup(options);
        const setup = new LocalSignerSetup(mapped, deriveSignerSetupPolicy(venue));
        let captured: string | undefined;
        class Capture {
          constructor(_venue: string, _options: string, json?: string) { captured = json; }
        }
        setup.construct(Capture as never, venue, '{}');
        expect(JSON.parse(captured!)).toEqual({ primary: {
          secret: row.scalar, secretEncoding: 'hex', trimSigning: false,
        } });
        expect(options.key).toBe('0x1111111111111111111111111111111111111111');
        expect(mapped.applicationId).toBe(options.applicationId);
        expect(mapped.vault).toBe(options.vault);
        expect(() => setup.construct(Capture as never, venue, '{}')).toThrow(/already been consumed/);
      });
    }
  }

  it('refuses any secondary or passphrase instead of fabricating an agent credential', () => {
    for (const extra of [
      { secondary_key: '' }, { secondary_secret: 'secret' }, { secondary_passphrase: '' },
      { passphrase: '' }, { localSigner: { secondary: { stableId: 'secondary' } } },
    ]) {
      expect(() => mapHyperliquidSignerSetup({ ...extra, key: 'account', secret: 'scalar' } as ExchangeOptions))
        .toThrow('Invalid Hyperliquid local signer setup');
    }
  });
});
