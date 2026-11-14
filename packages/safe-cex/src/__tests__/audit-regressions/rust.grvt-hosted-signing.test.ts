import { expect, it, vi } from 'vitest';

import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';

it('admits GRVT RemoteSigner while undeclared venues still refuse before signing', () => {
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  expect(deriveRemoteSigningPolicy('grvt')?.enabled).toBe(true);
  expect(() => assertBetaAccount('grvt', { remoteSigner })).not.toThrow();
  for (const name of ['krakenspot', 'mode'] as const) {
    expect(deriveRemoteSigningPolicy(name)).toBeUndefined();
    expect(() => assertBetaAccount(name, { key: 'fixture-private-key', remoteSigner })).toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent',
      message: expect.stringContaining("isn't available on the Hosted Server") }));
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it('uses a wallet provider for GRVT and refuses PayloadSigner custody', async () => {
  const { remoteSignerAlgorithms, createRemoteSignerProvider } = await import('../../exchanges/rust/remoteSignerProvider');
  expect(remoteSignerAlgorithms('grvt')).toEqual(['secp256k1Digest']);
  expect(() => createRemoteSignerProvider({ exchangeName: 'grvt', identities: {}, payloadSigner: vi.fn() })).toThrow();
});

it('rejects malformed and cross-identity wallet graphs before callback, and cancels late results', async () => {
  const { createRemoteSignerProvider } = await import('../../exchanges/rust/remoteSignerProvider');
  const sign = vi.fn();
  const primary = { credential: 'public-ref', signing: 'signing-ref', publicCredential: 'fixture-key' };
  const provider = createRemoteSignerProvider({ exchangeName: 'grvt', identities: { primary },
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } });
  const work = { receipt: {}, context: { venue: '', method: '', host: '', path: '' }, operations: [
    { kind: 'sha256' as const, payload: [{ kind: 'bytes' as const, bytes: [123, 125] }] },
    { kind: 'publicAddress' as const, key: primary.signing },
    { kind: 'sign' as const, algorithm: 'secp256k1Digest' as const, key: primary.signing,
      payload: [{ kind: 'bytes' as const, bytes: Array(32).fill(1) as number[] }] },
  ] };
  for (const operations of [work.operations.slice(1), [...work.operations, work.operations[1]],
    [work.operations[0], { ...work.operations[1], key: 'foreign' }, work.operations[2]]]) {
    await expect(provider({ ...work, operations }, { signal: new AbortController().signal })).rejects.toThrow('Remote signing request refused');
  }
  expect(sign).not.toHaveBeenCalled();
  const abort = new AbortController();
  let release!: (value: unknown) => void;
  sign.mockImplementationOnce(() => new Promise((resolve) => { release = resolve; }));
  const pending = provider(work, { signal: abort.signal });
  abort.abort();
  await expect(pending).rejects.toThrow('Remote signing canceled');
  release({ id: 'late', wsAuthSignature: 'late' });
});

it('refuses private GRVT PayloadSigner as unsupported without constructing a provider', async () => {
  const { DelegatedSignerSetup } = await import('../../exchanges/rust/delegatedSignerSetup');
  try {
    new DelegatedSignerSetup('grvt', { key: 'fixture-key', secret: '', payloadSigner: vi.fn() });
    throw new Error('must refuse');
  } catch (error) {
    expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
  }
});
