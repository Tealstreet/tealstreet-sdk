import { expect, it, vi } from 'vitest';

import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';

it('explicitly refuses Polymarket signature-only custody before callbacks or construction', () => {
  const sign = vi.fn();
  const remoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  for (const options of [{ remoteSigner }, { payloadSigner: sign }, { remoteSigner, payloadSigner: sign }]) {
    expect(() => assertBetaAccount('polymarket', { key: 'fixture-private-key', ...options })).toThrowError(expect.objectContaining({
      kind: 'unsupported', outcome: 'notSent',
      message: 'polymarket is unsupported in this build',
    }));
  }
  expect(sign).not.toHaveBeenCalled();
});

it('declares both Polymarket callback modes unsupported instead of relying on defaults', () => {
  expect(deriveRemoteSigningPolicy('polymarket')).toMatchObject({ enabled: false, payloadSupported: false });
});

it('preserves Polymarket beta admission for local and public-only accounts', () => {
  for (const options of [{ key: 'local-wallet' }, { publicDataOnly: true }]) {
    expect(() => assertBetaAccount('polymarket', options)).toThrow('polymarket is unsupported in this build');
  }
});
