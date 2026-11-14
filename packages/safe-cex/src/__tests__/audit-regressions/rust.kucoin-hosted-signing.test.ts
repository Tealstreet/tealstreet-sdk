import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';

import { expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';

it.each(['kucoin', 'kucoinspot'] as const)('%s: admits KuCoin remote custody while undeclared venues still refuse before signing', (venue) => {
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  expect(deriveRemoteSigningPolicy(venue)).toMatchObject({ enabled: true, payloadSupported: false, signatureEncoding: 'base64' });
  expect(deriveRemoteSigningPolicy(venue)?.protectedHeaders).toEqual(deriveRemoteSigningPolicy('kucoin')?.protectedHeaders);
  expect(() => assertBetaAccount(venue, { remoteSigner })).not.toThrow();
  for (const name of ['krakenspot', 'mode'] as const) {
    expect(deriveRemoteSigningPolicy(name)).toBeUndefined();
    const prettyName = exchangeMetadata[name].prettyName;
    expect(() => assertBetaAccount(name, { key: 'fixture-private-key', remoteSigner })).toThrow(
      `${prettyName} isn't available on the Hosted Server yet; turn Hosted Server off to trade it with your API key in this browser`,
    );
    expect(() => assertBetaAccount(name, { key: 'fixture-private-key', payloadSigner: vi.fn() })).toThrow(
      `${prettyName} delegated signing is unsupported in this build`,
    );
  }
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

it.each(['kucoin', 'kucoinspot'] as const)('%s: keeps unsupported private PayloadSigner refused without a local fallback', (venue) => {
  expect(
    () =>
      new DelegatedSignerSetup(venue, {
        key: 'fixture-key',
        secret: '',
        passphrase: 'fixture-pass',
        payloadSigner: vi.fn(),
      } as ExchangeOptions),
  ).toThrow(expect.objectContaining({ kind: 'unsupported', outcome: 'notSent' }));
});

const bytes = (text: string) => Array.from(new TextEncoder().encode(text));
const identity = {
  credential: 'ref:public',
  signing: 'ref:signing',
  passphrase: 'ref:passphrase',
  extra: { kucoinBroker: 'ref:broker' },
  publicCredential: 'fixture-key',
};
const payload = '1700000000123POST/api/v1/orders{"size":1,"memo":"é/+%"}';
const mac = (material: string, text: string) => createHmac('sha256', material).update(text).digest('base64');
const work = (broker = true): SignRequest => ({
  receipt: { stampMs: 1_700_000_000_123 },
  context: { venue: 'advisory', method: 'POST', host: '', path: '' },
  operations: [
    { kind: 'present', key: identity.credential },
    {
      kind: 'sign',
      algorithm: 'hmacSha256',
      key: identity.signing,
      payload: [{ kind: 'bytes', bytes: bytes(payload) }],
    },
    {
      kind: 'sign',
      algorithm: 'hmacSha256',
      key: identity.signing,
      payload: [{ kind: 'credential', key: identity.passphrase }],
    },
    ...(broker
      ? [
          {
            kind: 'sign' as const,
            algorithm: 'hmacSha256' as const,
            key: identity.extra.kucoinBroker,
            payload: [
              { kind: 'bytes' as const, bytes: bytes('1700000000123partner') },
              { kind: 'credential' as const, key: identity.credential },
            ],
          },
        ]
      : []),
  ],
});
const context = () => ({ signal: new AbortController().signal });
function rig(venue: 'kucoin' | 'kucoinspot') {
  const sign = vi.fn(async (request: RemoteSignerRequest) => {
    const descriptor = JSON.parse(request.wsAuth!.toSign) as { payload: string; brokerPrefix?: string };
    const macs = [mac('fixture-secret', descriptor.payload), mac('fixture-secret', 'fixture-passphrase')];
    if (descriptor.brokerPrefix) macs.push(mac('fixture-broker', descriptor.brokerPrefix + identity.publicCredential));
    return { id: request.id, wsAuthSignature: JSON.stringify({ credential: identity.publicCredential, macs }) };
  });
  const remoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const provider = createRemoteSignerProvider({
    exchangeName: venue,
    identities: { primary: identity },
    remoteSigner,
  });
  return { sign, provider };
}

it.each(['kucoin', 'kucoinspot'] as const)('%s: delegates one atomic KuCoin batch with exact payload/MAC bytes and no passphrase/broker material on the wire', async (venue) => {
  for (const broker of [false, true]) {
    const r = rig(venue);
    const result = await r.provider(work(broker), context());
    const macs = [mac('fixture-secret', payload), mac('fixture-secret', 'fixture-passphrase')];
    if (broker) macs.push(mac('fixture-broker', '1700000000123partnerfixture-key'));
    expect(result.values).toEqual([
      bytes('fixture-key'),
      ...macs.map((value) => Array.from(Buffer.from(value, 'base64'))),
    ]);
    expect(r.sign).toHaveBeenCalledTimes(1);
    expect(r.sign.mock.calls[0][0].exchangeName).toBe(venue);
    expect(r.sign.mock.calls[0][0].wsAuth).toEqual({
      outputFormat: 'kucoin-batch',
      toSign: JSON.stringify({
        slot: 'primary',
        payload,
        ...(broker ? { brokerPrefix: '1700000000123partner' } : {}),
      }),
    });
    const wire = JSON.stringify(r.sign.mock.calls);
    for (const material of ['fixture-secret', 'fixture-passphrase', 'fixture-broker', 'ref:'])
      expect(wire).not.toContain(material);
  }
});

it.each(['kucoin', 'kucoinspot'] as const)('%s: refuses unknown/cross-slot/partial batches before callbacks and redacts remote failures', async (venue) => {
  for (const mutate of [
    (request: SignRequest) => {
      request.operations.pop();
      request.operations.pop();
    },
    (request: SignRequest) => {
      request.operations[2] = { kind: 'present', key: identity.passphrase };
    },
    (request: SignRequest) => {
      const op = request.operations[2];
      if (op.kind === 'sign') op.payload = [{ kind: 'credential', key: 'foreign' }];
    },
    (request: SignRequest) => {
      const op = request.operations[3];
      if (op.kind === 'sign') op.key = identity.signing;
    },
  ]) {
    const r = rig(venue);
    const request = work();
    mutate(request);
    await expect(r.provider(request, context())).rejects.toThrow('Remote signing request refused');
    expect(r.sign).not.toHaveBeenCalled();
  }
  const r = rig(venue);
  r.sign.mockRejectedValueOnce(new Error('fixture-secret fixture-passphrase fixture-broker'));
  await expect(r.provider(work(), context())).rejects.toThrow(/^Remote signing request refused$/);
});

it.each(['kucoin', 'kucoinspot'] as const)('%s: refuses malformed/wrong-account responses and canceled late results atomically', async (venue) => {
  for (const answer of [
    '{}',
    JSON.stringify({ credential: 'wrong', macs: [mac('x', 'y'), mac('x', 'z')] }),
    JSON.stringify({ credential: 'fixture-key', macs: ['not-a-mac', 'not-a-mac'] }),
  ]) {
    const r = rig(venue);
    r.sign.mockImplementationOnce(async ({ id }) => ({ id, wsAuthSignature: answer }));
    await expect(r.provider(work(false), context())).rejects.toThrow('Remote signing request refused');
  }
  const r = rig(venue);
  const abort = new AbortController();
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  r.sign.mockImplementationOnce(async ({ id }) => {
    await held;
    return { id, wsAuthSignature: 'late' };
  });
  const pending = r.provider(work(), { signal: abort.signal });
  abort.abort();
  await expect(pending).rejects.toThrow('Remote signing canceled');
  release();
});
