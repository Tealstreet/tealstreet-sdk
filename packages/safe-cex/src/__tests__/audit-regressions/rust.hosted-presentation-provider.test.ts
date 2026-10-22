import type { SignRequest, SigningIdentities } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, RemoteSigner } from '../../types';

import { Buffer } from 'node:buffer';
import { describe, expect, it, vi } from 'vitest';

import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';

// Independent Python stdlib HMAC-SHA256 vector, retained as raw bytes via Node.
const mac = 'SL/xd+lRj5sirx9sxjcCAiy6l78Q3WwR8844XzqxcNs=';
const identity = {
  credential: 'ref:owned-primary-public', signing: 'ref:owned-primary-signing',
  publicCredential: 'public-identity',
};
const request = (): SignRequest => ({
  receipt: {}, context: { venue: 'advisory', method: 'POST', host: 'untrusted.invalid', path: '/' },
  operations: [{ kind: 'present', key: identity.credential }, {
    kind: 'sign', algorithm: 'hmacSha256', key: identity.signing,
    payload: [{ kind: 'bytes', bytes: Array.from(Buffer.from('1709209849123GET/api/account?x=%2B&x=second')) }],
  }],
});
const context = () => ({ signal: new AbortController().signal });
const names = ['okx', 'okxeu', 'okxdemo', 'okxeudemo', 'bitget', 'bitgetinverse', 'bitgetusdc'] as const;
function remote(): RemoteSigner {
  return { sign: vi.fn(async ({ id }) => ({ id, wsAuthSignature: mac })),
    isAvailable: () => true, onAvailabilityChange: () => () => {} };
}

describe('shared hosted provider presentation', () => {
  it.each(names)('%s requests the declared wire encoding and returns exact raw MAC bytes', async (exchangeName) => {
    const callback = remote();
    const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity }, remoteSigner: callback });
    const input = request();
    const result = await provider(input, context());
    expect(result.values).toEqual([Array.from(Buffer.from(identity.publicCredential)), Array.from(Buffer.from(mac, 'base64'))]);
    expect(callback.sign).toHaveBeenCalledWith({ id: expect.any(String), type: 'ws-auth', exchangeName,
      wsAuth: { toSign: '1709209849123GET/api/account?x=%2B&x=second', outputFormat: 'base64' } });
  });

  it.each(['okx', 'bitget'] as const)('%s validates required passphrase material in trusted callback setup', (exchangeName) => {
    const options = { key: 'public-identity', secret: '', remoteSigner: remote() } as ExchangeOptions;
    expect(() => new DelegatedSignerSetup(exchangeName as ExchangeName, options)).toThrow(/passphrase/);
  });

  it.each(['okx', 'bitget'] as const)('%s refuses every noncanonical or incorrectly sized base64 MAC', async (exchangeName) => {
    for (const malformed of [mac.slice(0, -1), `${mac}=`, `${mac}\n`, mac.replace('+', '-'),
      `${mac.slice(0, 42)}t=`, 'ab'.repeat(32), Buffer.alloc(31).toString('base64'), Buffer.alloc(33).toString('base64')]) {
      const callback = remote();
      vi.mocked(callback.sign).mockImplementation(async ({ id }) => ({ id, wsAuthSignature: malformed }));
      const provider = createRemoteSignerProvider({ exchangeName, identities: { primary: identity }, remoteSigner: callback });
      await expect(provider(request(), context())).rejects.toThrow('Remote signing request refused');
    }
  });

  function setup(exchangeName: ExchangeName, extra: Partial<ExchangeOptions> = {}) {
    const callback = vi.fn(async () => ({ signature: mac }));
    const options = { key: identity.publicCredential, secret: '', passphrase: 'fixture-presentation',
      payloadSigner: callback, ...extra } as ExchangeOptions;
    const capsule = new DelegatedSignerSetup(exchangeName, options);
    let encoded = '';
    class Constructor { constructor(_venue: string, json: string) { encoded = json; } }
    capsule.construct(Constructor as unknown as Parameters<DelegatedSignerSetup['construct']>[0], exchangeName, JSON.stringify(options));
    return { capsule, callback, options, encoded, ids: (JSON.parse(encoded) as { signing: SigningIdentities }).signing };
  }
  function ownedRequest(ids: SigningIdentities, slot: 'primary' | 'secondary' = 'primary'): SignRequest {
    const selected = ids[slot]!;
    const input = request();
    input.operations = [{ kind: 'present', key: selected.credential }, {
      kind: 'sign', algorithm: 'hmacSha256', key: selected.signing, payload: input.operations[1].kind === 'sign' ? input.operations[1].payload : [],
    }, { kind: 'present', key: selected.passphrase! }];
    return input;
  }

  it.each(['okx', 'bitget'] as const)('%s keeps material in provider custody and a scrubbed callback receiver', async (exchangeName) => {
    const s = setup(exchangeName);
    expect(s.encoded).not.toContain('fixture-presentation');
    expect(JSON.parse(s.encoded)).not.toHaveProperty('passphrase');
    expect(s.ids.primary?.passphrase).toEqual(expect.any(String));
    Reflect.set(s.options, 'passphrase', 'later-mutation');
    const result = await s.capsule.signer(ownedRequest(s.ids), context());
    expect(result.values[2]).toEqual(Array.from(Buffer.from('fixture-presentation')));
    expect(s.callback).toHaveBeenCalledWith(expect.objectContaining({ encoding: 'base64', key: identity.publicCredential }));
    expect(s.callback.mock.contexts[0]).toMatchObject({ key: identity.publicCredential });
    expect(s.callback.mock.contexts[0]).not.toHaveProperty('passphrase');
    expect(s.callback.mock.contexts[0]).not.toHaveProperty('secret');
    s.capsule.dispose();
    await expect(s.capsule.signer(ownedRequest(s.ids), context())).rejects.toThrow(/disposed/);
    expect(s.callback).toHaveBeenCalledTimes(1);
  });

  it.each(['okx', 'bitget'] as const)('%s aliases only absent secondary passphrases, including a primary-material-only setup', async (exchangeName) => {
    for (const key of [identity.publicCredential, '']) {
      for (const absent of [undefined, null]) {
        const s = setup(exchangeName, { key, secondary_key: 'read-public', secondary_passphrase: absent as unknown as string });
        if (s.ids.primary) expect(s.ids.secondary?.passphrase).toBe(s.ids.primary.passphrase);
        const result = await s.capsule.signer(ownedRequest(s.ids, 'secondary'), context());
        expect(result.values[0]).toEqual(Array.from(Buffer.from('read-public')));
        expect(result.values[2]).toEqual(Array.from(Buffer.from('fixture-presentation')));
      }
    }
    expect(() => setup(exchangeName, { secondary_key: 'read-public', secondary_passphrase: '' })).toThrow(/passphrase/);
    expect(() => setup(exchangeName, { publicDataOnly: true, secondary_key: 'read-public', secondary_passphrase: '' })).toThrow(/passphrase/);
    const own = setup(exchangeName, { secondary_key: 'read-public', secondary_passphrase: 'own-read-presentation' });
    expect(own.ids.secondary?.passphrase).not.toBe(own.ids.primary?.passphrase);
    expect((await own.capsule.signer(ownedRequest(own.ids, 'secondary'), context())).values[2])
      .toEqual(Array.from(Buffer.from('own-read-presentation')));
  });

  it.each(['okx', 'bitget'] as const)('%s never grants passphrase interpolation or unbound Present authority', async (exchangeName) => {
    const s = setup(exchangeName, { secondary_key: 'read-public', secondary_passphrase: 'owned-read-presentation' });
    const unbound = ownedRequest(s.ids);
    unbound.operations = [{ kind: 'present', key: s.ids.primary!.passphrase! }];
    await expect(s.capsule.signer(unbound, context())).rejects.toThrow('Remote signing request refused');
    const interpolated = ownedRequest(s.ids);
    const operation = interpolated.operations[1];
    if (operation.kind !== 'sign') throw new Error('Fixture shape');
    operation.payload = [{ kind: 'credential', key: s.ids.primary!.passphrase! }];
    await expect(s.capsule.signer(interpolated, context())).rejects.toThrow('Remote signing request refused');
    const crossSlot = ownedRequest(s.ids);
    crossSlot.operations[2] = { kind: 'present', key: s.ids.secondary!.passphrase! };
    await expect(s.capsule.signer(crossSlot, context())).rejects.toThrow('Remote signing request refused');
    const laterInvalid = ownedRequest(s.ids);
    laterInvalid.operations.push({ kind: 'sign', algorithm: 'hmacSha256', key: 'ref:foreign', payload: [] });
    await expect(s.capsule.signer(laterInvalid, context())).rejects.toThrow('Remote signing request refused');
    expect(s.callback).not.toHaveBeenCalled();
  });

  it.each(['okx', 'bitget'] as const)('%s remote wire supplies only the MAC and cannot select secondary authority', async (exchangeName) => {
    const callback = remote();
    const fallback = vi.fn(async () => ({ signature: mac }));
    const s = setup(exchangeName, { remoteSigner: callback, payloadSigner: fallback, secondary_key: 'read-public' });
    const result = await s.capsule.signer(ownedRequest(s.ids), context());
    expect(result.values[2]).toEqual(Array.from(Buffer.from('fixture-presentation')));
    expect(callback.sign).toHaveBeenCalledWith({ id: expect.any(String), type: 'ws-auth', exchangeName,
      wsAuth: { toSign: '1709209849123GET/api/account?x=%2B&x=second', outputFormat: 'base64' } });
    await expect(s.capsule.signer(ownedRequest(s.ids, 'secondary'), context())).rejects.toThrow('Remote signing request refused');
    expect(callback.sign).toHaveBeenCalledTimes(1);
    expect(fallback).not.toHaveBeenCalled();
  });

  it.each(['okx', 'bitget'] as const)('%s cannot publish a canceled passphrase batch after a late remote response', async (exchangeName) => {
    let resolve!: (value: { id: string; wsAuthSignature: string }) => void;
    const delayed = new Promise<{ id: string; wsAuthSignature: string }>((done) => { resolve = done; });
    const callback = remote();
    vi.mocked(callback.sign).mockReturnValue(delayed);
    const s = setup(exchangeName, { remoteSigner: callback });
    const abort = new AbortController();
    const input = ownedRequest(s.ids);
    // Sensitive presentation may precede an asynchronous Sign in an atomic batch.
    input.operations.unshift(input.operations.pop()!);
    const pending = s.capsule.signer(input, { signal: abort.signal });
    expect(callback.sign).toHaveBeenCalledTimes(1);
    abort.abort();
    await expect(pending).rejects.toThrow('Remote signing canceled');
    const id = vi.mocked(callback.sign).mock.calls[0][0].id;
    resolve({ id, wsAuthSignature: mac });
    await Promise.resolve();
    s.capsule.dispose();
    await expect(s.capsule.signer(input, context())).rejects.toThrow(/disposed/);
    expect(callback.sign).toHaveBeenCalledTimes(1);
  });
});
