import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';

import { expect, it, vi } from 'vitest';

import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { mapAsterDEXSignerSetup } from '../../exchanges/rust/asterdexSignerSetup';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from '../../exchanges/rust/remoteSignerProvider';

const primary = { credential: 'origin/credential', signing: 'origin/signing', publicCredential: 'fixture-key' };
const remote = (sign = vi.fn()) => ({ sign, isAvailable: () => true, onAvailabilityChange: () => () => {} });

it('admits Aster RemoteSigner and retains the undeclared-venue gate without callbacks', () => {
  const remoteSigner = remote();
  expect(deriveRemoteSigningPolicy('asterdex')?.enabled).toBe(true);
  expect(() => assertBetaAccount('asterdex', { remoteSigner })).not.toThrow();
  expect(() => assertBetaAccount('cryptocom', { remoteSigner })).toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent' }));
  expect(remoteSigner.sign).not.toHaveBeenCalled();
  expect(remoteSignerAlgorithms('asterdex')).toEqual(['hmacSha256', 'secp256k1Digest']);
});

it('V1 remote batches bind the selected device slot and public credential', async () => {
  const sign = vi.fn(async (request) => ({ id: request.id, wsAuthSignature: JSON.stringify({ credential: 'fixture-key', signature: 'ab'.repeat(32) }) }));
  const provider = createRemoteSignerProvider({ exchangeName: 'asterdex', identities: { primary }, remoteSigner: remote(sign) });
  const payload = 'symbol=BTCUSDT&recvWindow=20000&timestamp=1700000000123';
  const work: SignRequest = { receipt: { stampMs: 1700000000123 }, context: { venue: 'asterdex', method: 'GET', host: '', path: '' }, operations: [
    { kind: 'present', key: primary.credential },
    { kind: 'sign', key: primary.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: Array.from(new TextEncoder().encode(payload)) }] },
  ] };
  const result = await provider(work, { signal: new AbortController().signal });
  expect(result.values).toEqual([Array.from(new TextEncoder().encode('fixture-key')), Array(32).fill(171)]);
  expect(sign).toHaveBeenCalledWith(expect.objectContaining({ wsAuth: { toSign: JSON.stringify({ slot: 'primary', payload }), outputFormat: 'asterdex-hmac' } }));
  sign.mockImplementationOnce(async (request) => ({ id: request.id, wsAuthSignature: JSON.stringify({ credential: 'foreign', signature: 'ab'.repeat(32) }) }));
  await expect(provider(work, { signal: new AbortController().signal })).rejects.toThrow('Remote signing request refused');
  expect(JSON.stringify(sign.mock.calls)).not.toContain('origin/');
});

it('refuses V3 PayloadSigner with a typed reason before invoking it', () => {
  const payloadSigner = vi.fn();
  expect(() => new DelegatedSignerSetup('asterdex', { key: 'fixture-key', secret: '', extra: { asterdexApiVersion: 'v3' }, payloadSigner }))
    .toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent', message: expect.stringContaining('V3') }));
  expect(payloadSigner).not.toHaveBeenCalled();
});

it('resolves a keyless V3 API wallet and read agent from public addresses', () => {
  const key = `0x${'11'.repeat(20)}`;
  const read = `0x${'22'.repeat(20)}`;
  const options = { key, secret: '', applicationId: `0x${'33'.repeat(20)}`, secondary_key: read, remoteSigner: remote() };
  expect(mapAsterDEXSignerSetup(options).publicSetup).toEqual({ apiVersion: 'v3', signerAddress: key, readSignerAddress: read });
  expect(() => new DelegatedSignerSetup('asterdex', { ...options, remoteSigner: undefined, payloadSigner: vi.fn() }))
    .toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent' }));
});

it('refuses foreign graphs and raw wallet digests before callback, and cancels late answers without fallback or leaked errors', async () => {
  const sign = vi.fn();
  const provider = createRemoteSignerProvider({ exchangeName: 'asterdex', identities: { primary }, remoteSigner: remote(sign), payloadSigner: () => { throw new Error('must not fall back'); } });
  const work: SignRequest = { receipt: {}, context: { venue: '', method: '', host: '', path: '' }, operations: [
    { kind: 'present', key: primary.credential }, { kind: 'sign', key: primary.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: [97] }] },
  ] };
  for (const operations of [work.operations.slice(1), [work.operations[0], { ...work.operations[1], key: 'foreign' }],
    [work.operations[0], { ...work.operations[1], algorithm: 'secp256k1Digest' }]] as SignRequest['operations'][]) {
    await expect(provider({ ...work, operations }, { signal: new AbortController().signal })).rejects.toThrow('Remote signing request refused');
  }
  expect(sign).not.toHaveBeenCalled();
  sign.mockRejectedValueOnce(new Error('fixture-sensitive-error'));
  await expect(provider(work, { signal: new AbortController().signal })).rejects.toThrow(/^Remote signing request refused$/);
  let release!: (value: unknown) => void;
  sign.mockImplementationOnce(() => new Promise((resolve) => { release = resolve; }));
  const controller = new AbortController();
  const pending = provider(work, { signal: controller.signal }); controller.abort();
  await expect(pending).rejects.toThrow('Remote signing canceled'); release({ id: 'late', wsAuthSignature: 'late' });
});
