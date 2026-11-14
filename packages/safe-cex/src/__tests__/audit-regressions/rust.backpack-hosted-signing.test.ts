import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { AxiosHeaders } from 'axios';
import { expect, it, vi } from 'vitest';

import { prepareBackpackRequest } from '../../exchanges/rust/backpackHostedSigning';
import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';
import { endpoints, signature, STAMP } from './backpackHostedFixtures';

it('admits Backpack RemoteSigner while undeclared venues still refuse before callbacks', () => {
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  expect(() => assertBetaAccount('backpack', { remoteSigner })).not.toThrow();
  expect(deriveRemoteSigningPolicy('backpack')?.enabled).toBe(true);
  for (const name of ['mode', 'krakenspot'] as const) {
    expect(deriveRemoteSigningPolicy(name)).toBeUndefined();
    let refusal: unknown;
    try { assertBetaAccount(name, { key: 'fixture-private-key', remoteSigner }); } catch (error) { refusal = error; }
    expect(refusal).toMatchObject({ kind: 'unsupported', outcome: 'notSent',
      message: expect.stringContaining("isn't available on the Hosted Server") });
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it('keeps private Backpack PayloadSigner unsupported without local fallback', () => {
  expect(
    () =>
      new DelegatedSignerSetup('backpack', {
        key: 'fixture-key',
        secret: '',
        payloadSigner: vi.fn(),
      } as ExchangeOptions),
  ).toThrow();
});

const bytes = (text: string) => Array.from(new TextEncoder().encode(text));
const identity = { credential: 'ref:public', signing: 'ref:seed', publicCredential: 'fixture-key' };
const work = (payload: string): SignRequest => ({
  receipt: { stampMs: STAMP },
  context: { venue: 'advisory', method: 'GET', host: '', path: '' },
  operations: [
    { kind: 'present', key: identity.credential },
    { kind: 'sign', algorithm: 'ed25519', key: identity.signing, payload: [{ kind: 'bytes', bytes: bytes(payload) }] },
  ],
});
const context = () => ({ signal: new AbortController().signal });
function rig() {
  const sign = vi.fn(async (request: RemoteSignerRequest) => {
    const descriptor = JSON.parse(request.wsAuth!.toSign) as { slot: string; payload: string };
    return {
      id: request.id,
      wsAuthSignature: JSON.stringify({
        credential: identity.publicCredential,
        signature: signature(descriptor.payload),
      }),
    };
  });
  const remoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  return {
    sign,
    remoteSigner,
    provider: createRemoteSignerProvider({ exchangeName: 'backpack', remoteSigner, identities: { primary: identity } }),
  };
}

it('delegates every endpoint and private WS graph with exact Ed25519 bytes at fixed and corrected clocks', async () => {
  const r = rig();
  for (const [method, url, data, canonical] of endpoints) {
    for (const offset of [0, 1000]) {
      const stamp = STAMP + offset;
      const config = { method, url, data, headers: new AxiosHeaders() };
      const prepared = prepareBackpackRequest(config, stamp)!;
      const expected = `${canonical}&timestamp=${stamp}&window=60000`;
      expect(prepared.payload, url).toBe(expected);
      const result = await r.provider({ ...work(expected), receipt: { stampMs: stamp } }, context());
      expect(result.values).toEqual([bytes('fixture-key'), Array.from(Buffer.from(signature(expected), 'base64'))]);
      expect(r.sign).toHaveBeenLastCalledWith(
        expect.objectContaining({
          wsAuth: {
            outputFormat: 'backpack-ed25519',
            toSign: JSON.stringify({ slot: 'primary', payload: expected }),
          },
        }),
      );
    }
  }
  const payload = `instruction=subscribe&timestamp=${STAMP}&window=5000`;
  expect((await r.provider(work(payload), context())).values[1]).toEqual(
    Array.from(Buffer.from(signature(payload), 'base64')),
  );
  expect(JSON.stringify(r.sign.mock.calls)).not.toContain('ref:');
});

it('refuses foreign, partial and wrong-algorithm graphs before asking the device', async () => {
  for (const mutate of [
    (request: SignRequest) => {
      request.operations.push(request.operations[1]);
    },
    (request: SignRequest) => {
      const op = request.operations[1];
      if (op.kind === 'sign') op.key = 'foreign';
    },
    (request: SignRequest) => {
      const op = request.operations[1];
      if (op.kind === 'sign') op.algorithm = 'hmacSha256';
    },
    (request: SignRequest) => {
      const op = request.operations[1];
      if (op.kind === 'sign') op.payload = [{ kind: 'credential', key: identity.credential }];
    },
  ]) {
    const r = rig();
    const request = work('fixture');
    mutate(request);
    await expect(r.provider(request, context())).rejects.toThrow('Remote signing request refused');
    expect(r.sign).not.toHaveBeenCalled();
  }
});

it('refuses malformed signatures and wrong-account answers atomically, with redacted errors', async () => {
  const r = rig();
  for (const answer of [
    '{}',
    JSON.stringify({ credential: 'foreign', signature: signature('fixture') }),
    JSON.stringify({ credential: 'fixture-key', signature: Buffer.alloc(32).toString('base64') }),
    JSON.stringify({ credential: 'fixture-key', signature: signature('fixture'), extra: true }),
  ]) {
    r.sign.mockImplementationOnce(async ({ id }) => ({ id, wsAuthSignature: answer }));
    await expect(r.provider(work('fixture'), context())).rejects.toThrow(/^Remote signing request refused$/);
  }
  r.sign.mockRejectedValueOnce(new Error('fixture-secret fixture-seed ref:seed'));
  await expect(r.provider(work('fixture'), context())).rejects.toThrow(/^Remote signing request refused$/);
  r.remoteSigner.isAvailable = () => false;
  // Availability is read on each guarded device operation.
  await expect(r.provider(work('fixture'), context())).rejects.toThrow('Remote signer unavailable');
});

it('cancels late device replies without returning partial signatures', async () => {
  const r = rig();
  const abort = new AbortController();
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  r.sign.mockImplementationOnce(async ({ id }) => {
    await held;
    return { id, wsAuthSignature: 'late' };
  });
  const pending = r.provider(work('fixture'), { signal: abort.signal });
  abort.abort();
  await expect(pending).rejects.toThrow('Remote signing canceled');
  release();
});
