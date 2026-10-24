import { describe, expect, it } from 'vitest';
import type { ExchangeOptions } from '../../types';
import { deriveSignerSetupPolicy } from '../../exchanges/rust/construction';
import { LocalSignerSetup, withoutSigningCredentials } from '../../exchanges/rust/localSignerSetup';

describe('KuCoin trusted broker setup', () => {
  it.each(['kucoin', 'kucoinspot'] as const)('consumes %s broker material once and retains metadata', (venue) => {
    const input = {
      key: 'fixture-primary-key', secret: 'fixture-primary-secret', passphrase: 'fixture-passphrase',
      secondary_key: 'fixture-read-key', secondary_secret: 'fixture-read-secret',
      extra: { kucoinBroker: { partner: ' partner ', brokerKey: '\uFEFFfixture-broker\uFEFF', brokerName: ' name ', verify: false } },
    } as ExchangeOptions;
    const generic = withoutSigningCredentials(input);
    expect(JSON.stringify(generic)).not.toContain('fixture-broker');
    expect(generic.extra?.kucoinBroker).toEqual({ partner: ' partner ', brokerName: ' name ', verify: false });
    let setup: Record<string, unknown> = {};
    class Engine {
      constructor(_venue: string, _options: string, json?: string) { setup = JSON.parse(json!); }
    }
    const capsule = new LocalSignerSetup(input, deriveSignerSetupPolicy(venue));
    capsule.construct(Engine as never, venue, JSON.stringify(generic));
    expect(setup.primary).toMatchObject({ extra: { kucoinBroker: { value: input.extra!.kucoinBroker!.brokerKey, encoding: 'utf8' } } });
    expect(setup.secondary).not.toHaveProperty('extra');
    expect(input.extra?.kucoinBroker?.brokerKey).toBe('\uFEFFfixture-broker\uFEFF');
    expect(() => capsule.construct(Engine as never, venue, '{}')).toThrow(/consumed/);
  });
});
