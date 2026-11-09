import type { SignRequest, SigningIdentities, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';
import { expect, it, vi } from 'vitest';

import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from '../../exchanges/rust/remoteSignerProvider';

const bytes = (text: string) => Array.from(new TextEncoder().encode(text));
const identity = { credential: 'ref:public', signing: 'ref:hmac', passphrase: 'ref:pass',
  extra: { zklinkSeed: 'ref:zk' }, publicCredential: 'fixture-public', passphraseValue: 'fixture-pass' };
const contract = [254, 0, 0, 0, 1, ...Array<number>(31).fill(0)];
const zkSignature = '01'.repeat(64);
const hmac = (text: string) => createHmac('sha256', Buffer.from('fixture-material').toString('base64')).update(text).digest('base64');
const work = (order: boolean): SignRequest => ({
  receipt: { stampMs: 1_700_000_000_123 },
  context: { venue: 'advisory', method: '', host: '', path: '' },
  operations: [
    ...(order ? [{ kind: 'sign' as const, algorithm: 'zkLinkContract' as const,
      key: identity.extra.zklinkSeed, payload: [{ kind: 'bytes' as const, bytes: contract }] }] : []),
    { kind: 'present', key: identity.credential },
    { kind: 'present', key: identity.passphrase },
    { kind: 'sign', algorithm: 'hmacSha256', key: identity.signing,
      payload: order ? [{ kind: 'bytes', bytes: bytes('1700000000123POST/api/v3/order{signature=') },
        { kind: 'result', index: 0, encoding: 'hex' }, { kind: 'bytes', bytes: bytes('}') }]
        : [{ kind: 'bytes', bytes: bytes('1700000000123GET/ws/accounts') }] },
  ],
});
const context = () => ({ signal: new AbortController().signal });
function rig() {
  const sign = vi.fn(async (request: RemoteSignerRequest) => {
    const descriptor = JSON.parse(request.wsAuth!.toSign) as { contracts: number[][]; payload: Array<{ bytes?: number[]; contract?: number }> };
    const payload = descriptor.payload.map((part) => part.bytes ? Buffer.from(part.bytes).toString('utf8') : zkSignature).join('');
    return { id: request.id, wsAuthSignature: JSON.stringify({ credential: identity.publicCredential,
      contracts: descriptor.contracts.map(() => zkSignature), signature: hmac(payload) }) };
  });
  const remoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  return { sign, remoteSigner, provider: createRemoteSignerProvider({ exchangeName: 'apexomni',
    identities: { primary: identity }, remoteSigner }) };
}

it('declares Apex Omni RemoteSigner admission and the complete HMAC/zkLink capability set', () => {
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  expect(deriveRemoteSigningPolicy('apexomni')).toMatchObject({ enabled: true, payloadSupported: false });
  expect(remoteSignerAlgorithms('apexomni')).toEqual(['hmacSha256', 'zkLinkContract']);
  expect(() => assertBetaAccount('apexomni', { remoteSigner })).not.toThrow();
  expect(() => assertBetaAccount('polymarket', { remoteSigner })).toThrow();
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it.each([false, true])('delegates exact WS/REST and contract-dependent HMAC bytes atomically (order=%s)', async (order) => {
  const r = rig();
  const request = work(order);
  const output = await r.provider(request, context());
  const payload = order ? `1700000000123POST/api/v3/order{signature=${zkSignature}}` : '1700000000123GET/ws/accounts';
  expect(output).toEqual({ receipt: request.receipt, values: [
    ...(order ? [Array<number>(64).fill(1)] : []), bytes('fixture-public'), bytes('fixture-pass'), Array.from(Buffer.from(hmac(payload), 'base64')),
  ] });
  expect(r.sign).toHaveBeenCalledOnce();
  expect(r.sign.mock.calls[0][0].wsAuth?.outputFormat).toBe('apexomni-batch');
  const wire = JSON.stringify(r.sign.mock.calls);
  for (const material of ['fixture-material', 'fixture-pass', 'ref:']) expect(wire).not.toContain(material);
});

it('installs owned opaque refs and refuses private PayloadSigner and secondary custody before callbacks', () => {
  const r = rig();
  const setup = new DelegatedSignerSetup('apexomni', { key: 'fixture-public', secret: '', passphrase: 'fixture-pass', remoteSigner: r.remoteSigner });
  expect(setup.signerAlgorithms).toEqual(['hmacSha256', 'zkLinkContract']);
  setup.dispose();
  for (const options of [
    { key: 'fixture-public', secret: '', passphrase: 'fixture-pass', payloadSigner: vi.fn() },
    { key: 'fixture-public', secret: '', passphrase: 'fixture-pass', remoteSigner: r.remoteSigner, secondary_key: 'secondary' },
  ]) {
    let error: unknown;
    try { new DelegatedSignerSetup('apexomni', options); } catch (caught) { error = caught; }
    expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
  }
  expect(r.sign).not.toHaveBeenCalled();
});

it('rejects foreign refs, graph substitutions, malformed replies and provider errors without fallback', async () => {
  for (const mutate of [
    (request: SignRequest) => { request.operations[0] = { kind: 'present', key: 'foreign' }; },
    (request: SignRequest) => { const op = request.operations[0]; if (op.kind === 'sign') op.key = identity.signing; },
    (request: SignRequest) => { const op = request.operations[3]; if (op.kind === 'sign') op.payload = [{ kind: 'result', index: 1, encoding: 'hex' }]; },
  ]) {
    const r = rig(); const request = work(true); mutate(request);
    await expect(r.provider(request, context())).rejects.toThrow(/^Remote signing request refused$/);
    expect(r.sign).not.toHaveBeenCalled();
  }
  for (const response of ['{}', JSON.stringify({ credential: 'foreign', contracts: [], signature: hmac('x') }),
    JSON.stringify({ credential: identity.publicCredential, contracts: ['00'], signature: hmac('x') })]) {
    const r = rig(); r.sign.mockImplementationOnce(async (input) => ({ id: input.id, wsAuthSignature: response }));
    await expect(r.provider(work(true), context())).rejects.toThrow(/^Remote signing request refused$/);
  }
  const r = rig(); r.sign.mockRejectedValueOnce(new Error('fixture-material fixture-pass'));
  await expect(r.provider(work(true), context())).rejects.toThrow(/^Remote signing request refused$/);
});

it('cancels a delayed remote batch without accepting its late result', async () => {
  const r = rig(); let finish!: (result: Awaited<ReturnType<typeof r.sign>>) => void;
  r.sign.mockImplementationOnce((input) => new Promise((resolve) => { finish = (answer) => resolve({ ...answer, id: input.id }); }));
  const abort = new AbortController(); const pending = r.provider(work(true), { signal: abort.signal });
  abort.abort(); await expect(pending).rejects.toThrow('Remote signing canceled');
  finish({ id: '', wsAuthSignature: '{}' });
});

it('drops user A\'s deferred batch after disposal while user B signs with its own identity', async () => {
  const answer = (request: RemoteSignerRequest, key: string) => ({ id: request.id,
    wsAuthSignature: JSON.stringify({ credential: key, contracts: [zkSignature], signature: hmac(key) }) });
  let finishA: (() => void) | undefined;
  const signA = vi.fn((request: RemoteSignerRequest) => new Promise<ReturnType<typeof answer>>((resolve) => {
    finishA = () => resolve(answer(request, 'user-a'));
  }));
  const signB = vi.fn(async (request: RemoteSignerRequest) => answer(request, 'user-b'));
  const account = (key: string, sign: typeof signA | typeof signB) => {
    const setup = new DelegatedSignerSetup('apexomni', { key, secret: '', passphrase: `${key}-pass`,
      remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } });
    let identities: SigningIdentities = {};
    class Capture {
      constructor(_venue: string, json: string) { identities = JSON.parse(json).signing; }
    }
    setup.construct(Capture as unknown as new (venue: string, json: string) => WasmEngineLike, 'apexomni', '{}');
    const primary = identities.primary;
    if (!primary?.passphrase || !primary.extra?.zklinkSeed) throw new Error('Missing owned Apex refs');
    const refs = new Map([[identity.credential, primary.credential], [identity.signing, primary.signing],
      [identity.passphrase, primary.passphrase], [identity.extra.zklinkSeed, primary.extra.zklinkSeed]]);
    const request = work(true);
    for (const operation of request.operations) {
      if (operation.kind === 'present' || operation.kind === 'sign')
        operation.key = refs.get(operation.key) ?? operation.key;
    }
    return { setup, request };
  };
  const a = account('user-a', signA);
  let b: ReturnType<typeof account> | undefined;
  try {
    const pendingA = a.setup.signer(a.request, context());
    const canceledA = expect(pendingA).rejects.toThrow('Remote signing canceled');
    expect(signA).toHaveBeenCalledOnce();
    a.setup.dispose();
    b = account('user-b', signB);
    const resultB = await b.setup.signer(b.request, context());
    await canceledA;
    if (!finishA) throw new Error('User A did not defer');
    finishA();
    await Promise.resolve();
    expect(resultB.values[1]).toEqual(bytes('user-b'));
    expect(resultB.values[2]).toEqual(bytes('user-b-pass'));
    expect(signB).toHaveBeenCalledOnce();
    expect(signA).toHaveBeenCalledOnce();
    await expect(b.setup.signer(a.request, context())).rejects.toThrow('Remote signing request refused');
    await expect(a.setup.signer(a.request, context())).rejects.toMatchObject({ outcome: 'notSent' });
    expect(signB).toHaveBeenCalledOnce();
  } finally {
    a.setup.dispose();
    b?.setup.dispose();
  }
});
