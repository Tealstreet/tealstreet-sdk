import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';

import { expect, it, vi } from 'vitest';

import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { mapAsterDEXSignerSetup } from '../../exchanges/rust/asterdexSignerSetup';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from '../../exchanges/rust/remoteSignerProvider';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const primary = { credential: 'origin/credential', signing: 'origin/signing', publicCredential: 'fixture-key' };
const remote = (sign = vi.fn()) => ({ sign, isAvailable: () => true, onAvailabilityChange: () => () => {} });

it('admits Aster RemoteSigner and retains the undeclared-venue gate without callbacks', () => {
  const remoteSigner = remote();
  const options = { key: 'fixture-key', secret: '', remoteSigner };
  expect(deriveRemoteSigningPolicy('asterdex')?.enabled).toBe(true);
  expect(() => assertBetaAccount('asterdex', options)).not.toThrow();
  expect(() => assertBetaAccount('cryptocom', options)).not.toThrow();
  expect(() => assertBetaAccount('kraken', options)).not.toThrow();
  expect(deriveRemoteSigningPolicy('krakenspot')).toBeUndefined();
  expect(() => assertBetaAccount('krakenspot', options)).toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent' }));
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

it.each<{ version: 'v1' | 'v3' }>([{ version: 'v1' }, { version: 'v3' }])(
  'public-only $version construction ignores unused delegated signers', ({ version }) => {
    for (const key of ['', 'fixture-key']) {
      const payloadSigner = vi.fn();
      const remoteSigner = remote();
      for (const callbacks of [{ payloadSigner }, { remoteSigner }, { remoteSigner, payloadSigner }]) {
        const exchange = new RustEngineExchange('asterdex', { key, secret: '', publicDataOnly: true,
          extra: { asterdexApiVersion: version }, ...callbacks }, new DefaultStore());
        try {
          expect(exchange.isPublicDataOnly).toBe(true);
          expect(payloadSigner).not.toHaveBeenCalled();
          expect(remoteSigner.sign).not.toHaveBeenCalled();
        } finally { exchange.dispose(); }
      }
    }
  },
);

it.each([undefined, false])('refuses private V3 PayloadSigner with publicDataOnly=%s before invoking it', (publicDataOnly) => {
  const payloadSigner = vi.fn();
  const options = { key: 'fixture-key', secret: '', publicDataOnly, payloadSigner };
  expect(() => new RustEngineExchange('asterdex', { ...options, extra: { asterdexApiVersion: 'v3' } }, new DefaultStore()))
    .toThrowError(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent',
      message: 'AsterDEX V3 requires RemoteSigner API-wallet custody; PayloadSigner supports V1 HMAC only' }));
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
  const credential: SignRequest['operations'][number] = { kind: 'present', key: primary.credential };
  const hmac: SignRequest['operations'][number] = { kind: 'sign', key: primary.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: [97] }] };
  const work: SignRequest = { receipt: {}, context: { venue: '', method: '', host: '', path: '' }, operations: [
    credential, hmac,
  ] };
  const foreignGraphs: SignRequest['operations'][] = [[hmac], [credential, { ...hmac, key: 'foreign' }],
    [credential, { ...hmac, algorithm: 'secp256k1Digest' }]];
  for (const operations of foreignGraphs) {
    await expect(provider({ ...work, operations }, { signal: new AbortController().signal })).rejects.toThrow('Remote signing request refused');
  }
  expect(sign).not.toHaveBeenCalled();
  sign.mockRejectedValueOnce(new Error('fixture-sensitive-error'));
  await expect(provider(work, { signal: new AbortController().signal })).rejects.toThrow(/^Remote signing request refused$/);
  const late = Promise.withResolvers<unknown>();
  sign.mockImplementationOnce(() => late.promise);
  const controller = new AbortController();
  const pending = provider(work, { signal: controller.signal }); controller.abort();
  await expect(pending).rejects.toThrow('Remote signing canceled'); late.resolve({ id: 'late', wsAuthSignature: 'late' });
});
