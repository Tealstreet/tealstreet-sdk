import type { SignRequest, SignResult } from '@tealstreet/safe-cex-wasm/host';
import type { PayloadSigner, RemoteSigner } from '../../types';

import { createHash, createHmac } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';

import { prepareBitunixRequest } from '../../exchanges/bitunix/bitunix.signing';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from '../../exchanges/rust/remoteSignerProvider';

const primary = { credential: 'owned/primary/public', signing: 'owned/primary/hmac', publicCredential: 'public+é' };
const secondary = { credential: 'owned/read/public', signing: 'owned/read/hmac', publicCredential: 'read-public' };
const identities = { primary, secondary };
const hex = 'aB'.repeat(32);
const context = () => ({ signal: new AbortController().signal });
const request = (slot = primary): SignRequest => ({
  receipt: { stampMs: 1700000000123, expiryMs: 1700000010123 },
  context: { venue: 'untrusted', method: 'POST', host: 'untrusted.invalid', path: '/not-authority' },
  operations: [
    { kind: 'present', key: slot.credential },
    { kind: 'sign', algorithm: 'hmacSha256', key: slot.signing, payload: [
      { kind: 'bytes', bytes: [0x31, 0x32, 0x33, 0] },
      { kind: 'credential', key: slot.credential },
      { kind: 'bytes', bytes: [0xef, 0xbb, 0xbf, 0x26, 0x25, 0x32, 0x42] },
    ] },
  ],
});
function remote(sign?: RemoteSigner['sign']) {
  let listener: ((available: boolean) => void) | undefined;
  const unsubscribe = vi.fn(() => { listener = undefined; });
  const value: RemoteSigner = {
    accountName: 'account-one',
    sign: vi.fn(sign ?? (async ({ id }) => ({ id, wsAuthSignature: hex }))),
    isAvailable: vi.fn(() => true),
    onAvailabilityChange: vi.fn((callback) => { listener = callback; return unsubscribe; }),
  };
  return { value, unsubscribe, notify: (available: boolean) => listener?.(available) };
}
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
};

describe('standalone account-owned remote primitive provider', () => {
  it('preserves the exact old envelope, preimage, result positions and receipt without context authority', async () => {
    const r = remote();
    const fallback = vi.fn();
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, remoteSigner: r.value, payloadSigner: fallback });
    const input = request();
    const output = await signer(input, context());
    expect(r.value.sign).toHaveBeenCalledWith({
      id: expect.any(String), type: 'ws-auth', exchangeName: 'binance', accountName: 'account-one',
      wsAuth: { toSign: '123\0public+é\uFEFF&%2B', outputFormat: 'hex' },
    });
    expect(output).toEqual({ values: [Array.from(new TextEncoder().encode(primary.publicCredential)), Array(32).fill(0xab)], receipt: input.receipt });
    expect(output.receipt).not.toBe(input.receipt);
    expect(fallback).not.toHaveBeenCalled();
    expect(r.unsubscribe).toHaveBeenCalledTimes(1);
    await signer(request(), context());
    const calls = vi.mocked(r.value.sign).mock.calls;
    expect(calls[0][0].id).not.toBe(calls[1][0].id);
  });

  it('presents owned credentials without consulting the remote provider, including secondary', async () => {
    const r = remote();
    vi.mocked(r.value.isAvailable).mockReturnValue(false);
    const signer = createRemoteSignerProvider({ exchangeName: 'bybit', identities, remoteSigner: r.value });
    const input = request(secondary);
    input.operations = [{ kind: 'present', key: secondary.credential }, { kind: 'present', key: primary.credential }];
    expect((await signer(input, context())).values.map((value) => new TextDecoder().decode(new Uint8Array(value))))
      .toEqual(['read-public', 'public+é']);
    expect(r.value.sign).not.toHaveBeenCalled();
  });

  it('refuses required secondary signing and mixed-slot credential interpolation before any callback', async () => {
    const r = remote();
    const fallback = vi.fn();
    const signer = createRemoteSignerProvider({ exchangeName: 'bybit', identities, remoteSigner: r.value, payloadSigner: fallback });
    await expect(signer(request(secondary), context())).rejects.toThrow('Remote signing request refused');
    const mixed = request();
    const op = mixed.operations[1];
    if (op.kind !== 'sign') throw new Error('fixture');
    op.payload = [{ kind: 'credential', key: secondary.credential }];
    await expect(signer(mixed, context())).rejects.toThrow('Remote signing request refused');
    expect(r.value.sign).not.toHaveBeenCalled();
    expect(fallback).not.toHaveBeenCalled();
  });

  it('routes PayloadSigner secondary by explicit public key and preserves bounded headers last-wins', async () => {
    const payloadSigner = vi.fn(async () => ({ signature: hex, headers: { 'X-Value': 'old', 'x-value': 'last\tvalue', 'X-Other': 'é' } }));
    const signer = createRemoteSignerProvider({ exchangeName: 'bybit', identities, payloadSigner });
    const input = request(secondary);
    expect(await signer(input, context())).toEqual({
      values: [Array.from(new TextEncoder().encode(secondary.publicCredential)), Array(32).fill(0xab)],
      receipt: input.receipt, headers: [['x-value', 'last\tvalue'], ['X-Other', 'é']],
    });
    expect(payloadSigner).toHaveBeenCalledWith({ exchangeName: 'bybit', key: 'read-public', payload: '123\0read-public\uFEFF&%2B', scheme: 'hmac-sha256', encoding: 'hex' });
  });

  it('joins bytes before decoding split UTF-8 and retains a leading BOM', async () => {
    const callback = vi.fn(async () => ({ signature: hex }));
    const input = request();
    input.operations = [{ kind: 'sign', algorithm: 'hmacSha256', key: primary.signing,
      payload: [{ kind: 'bytes', bytes: [239, 187, 191, 195] }, { kind: 'bytes', bytes: [169] }] }];
    await createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: callback })(input, context());
    expect(callback.mock.calls[0]).toEqual([{ exchangeName: 'binance', key: 'public+é', payload: '\uFEFFé', scheme: 'hmac-sha256', encoding: 'hex' }]);
  });

  it.each(['wrong-id', 'error', 'nonstring-error', 'short', 'nonhex', 'absent', 'null', 'exception'])('rejects remote %s with closed errors and no fallback', async (kind) => {
    const r = remote(async ({ id }) => {
      if (kind === 'exception') throw new Error('sensitive callback exception');
      if (kind === 'null') return null as never;
      return { id: kind === 'wrong-id' ? 'different' : id,
        ...(kind === 'error' ? { error: 'sensitive venue message' } : kind === 'nonstring-error' ? { error: 123 as never } : {}),
        wsAuthSignature: kind === 'short' ? 'ab' : kind === 'nonhex' ? 'z'.repeat(64) : kind === 'absent' ? undefined : hex };
    });
    const fallback = vi.fn();
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, remoteSigner: r.value, payloadSigner: fallback });
    await expect(signer(request(), context())).rejects.toThrow(/^Remote signing request refused$/);
    expect(fallback).not.toHaveBeenCalled();
    expect(r.unsubscribe).toHaveBeenCalledTimes(1);
  });

  it.each(['null', 'nonhex', 'exception'])('rejects malformed PayloadSigner %s without exposing callback text', async (kind) => {
    const callback: PayloadSigner = async () => {
      if (kind === 'exception') throw new Error('sensitive payload exception');
      return kind === 'null' ? null as never : { signature: 'x'.repeat(64) };
    };
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: callback });
    await expect(signer(request(), context())).rejects.toThrow(/^Remote signing request refused$/);
  });

  it('omits absent accountName and settles even when availability cleanup throws', async () => {
    const r = remote();
    delete r.value.accountName;
    r.unsubscribe.mockImplementation(() => { throw new Error('sensitive cleanup exception'); });
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, remoteSigner: r.value });
    expect((await signer(request(), context())).values).toHaveLength(2);
    expect(vi.mocked(r.value.sign).mock.calls[0][0]).not.toHaveProperty('accountName');
    expect(r.unsubscribe).toHaveBeenCalledTimes(1);
  });

  it('preserves legacy typed empty-error responses with a valid exact-id signature', async () => {
    const r = remote(async ({ id }) => ({ id, error: '', wsAuthSignature: hex }));
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, remoteSigner: r.value });
    expect((await signer(request(), context())).values[1]).toEqual(Array(32).fill(0xab));
  });

  it('matches shared bounds of 32 operations and 256 payload parts', async () => {
    const callback = vi.fn(async () => ({ signature: hex }));
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: callback });
    const input = request();
    input.operations = Array.from({ length: 32 }, () => ({ kind: 'present', key: primary.credential }));
    expect((await signer(input, context())).values).toHaveLength(32);
    const payload = Array.from({ length: 256 }, () => ({ kind: 'bytes' as const, bytes: [65] }));
    input.operations = [{ kind: 'sign', algorithm: 'hmacSha256', key: primary.signing, payload }];
    await signer(input, context());
    expect(callback).toHaveBeenCalledWith({ exchangeName: 'binance', key: 'public+é', payload: 'A'.repeat(256), scheme: 'hmac-sha256', encoding: 'hex' });
    payload.push({ kind: 'bytes', bytes: [65] });
    await expect(signer(input, context())).rejects.toThrow('Remote signing request refused');
    expect(callback).toHaveBeenCalledTimes(1);
  });

  it('refuses unavailable providers and cleans up synchronous availability notifications', async () => {
    const r = remote();
    vi.mocked(r.value.isAvailable).mockReturnValue(false);
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, remoteSigner: r.value });
    await expect(signer(request(), context())).rejects.toThrow('Remote signer unavailable');
    expect(r.value.sign).not.toHaveBeenCalled();
    vi.mocked(r.value.isAvailable).mockReturnValue(true);
    vi.mocked(r.value.onAvailabilityChange).mockImplementation((callback) => { callback(false); return r.unsubscribe; });
    await expect(signer(request(), context())).rejects.toThrow('Remote signer unavailable');
    expect(r.value.sign).not.toHaveBeenCalled();
    expect(r.unsubscribe).toHaveBeenCalledTimes(1);
  });

  it.each(['abort', 'availability'])('settles promptly on %s and ignores late responses without fallback or leaked listeners', async (event) => {
    const pending = deferred<{ id: string; wsAuthSignature: string }>();
    const r = remote(() => pending.promise);
    const fallback = vi.fn();
    const controller = new AbortController();
    const remove = vi.spyOn(controller.signal, 'removeEventListener');
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, remoteSigner: r.value, payloadSigner: fallback });
    const result = signer(request(), { signal: controller.signal });
    const expected = expect(result).rejects.toThrow(event === 'abort' ? 'Remote signing canceled' : 'Remote signer unavailable');
    if (event === 'abort') controller.abort();
    else r.notify(false);
    await expected;
    const id = vi.mocked(r.value.sign).mock.calls[0][0].id;
    pending.resolve({ id, wsAuthSignature: hex });
    await Promise.resolve();
    expect(r.unsubscribe).toHaveBeenCalledTimes(1);
    expect(remove).toHaveBeenCalledWith('abort', expect.any(Function));
    expect(fallback).not.toHaveBeenCalled();
  });

  it('honors abort before dispatch and after PayloadSigner completion', async () => {
    const callback = vi.fn(async () => ({ signature: hex }));
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: callback });
    const controller = new AbortController();
    controller.abort();
    await expect(signer(request(), { signal: controller.signal })).rejects.toThrow('Remote signing canceled');
    expect(callback).not.toHaveBeenCalled();
    const next = new AbortController();
    callback.mockImplementation(async () => { next.abort(); return { signature: hex }; });
    await expect(signer(request(), { signal: next.signal })).rejects.toThrow('Remote signing canceled');
  });

  it('refuses bad refs, unsupported algorithms/result parts, malformed bytes and oversize batches before callbacks', async () => {
    const callback = vi.fn(async () => ({ signature: hex }));
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: callback });
    const invalid: SignRequest['operations'][] = [
      [{ kind: 'present', key: primary.signing }],
      [{ kind: 'present', key: 'another-account/public' }],
      [{ kind: 'sign', key: 'another-account/hmac', algorithm: 'hmacSha256', payload: [] }],
      [{ kind: 'sha256', payload: [] }],
      [{ kind: 'sign', key: primary.signing, algorithm: 'ed25519', payload: [] }],
      [{ kind: 'sign', key: primary.signing, algorithm: 'hmacSha256', payload: [{ kind: 'result', index: 0, encoding: 'raw' }] }],
      ...[[256], [-1], [1.5], [0xc3], Array(3), Array(1_048_577).fill(0)].map((bytes): SignRequest['operations'] =>
        [{ kind: 'sign', key: primary.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes }] }]),
      Array(33).fill({ kind: 'present', key: primary.credential }),
    ];
    for (const operations of invalid) {
      await expect(signer({ ...request(), operations }, context())).rejects.toThrow('Remote signing request refused');
    }
    expect(callback).not.toHaveBeenCalled();
  });

  it('rejects late holes in operations or payloads before invoking an earlier valid HMAC callback', async () => {
    const callback = vi.fn(async () => ({ signature: hex }));
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: callback });
    const operations = request().operations;
    operations.length = 3;
    await expect(signer({ ...request(), operations }, context())).rejects.toThrow('Remote signing request refused');
    const next = request();
    const malformed = request().operations[1];
    if (malformed.kind !== 'sign') throw new Error('fixture');
    malformed.payload.length += 1;
    next.operations.push(malformed);
    await expect(signer(next, context())).rejects.toThrow('Remote signing request refused');
    expect(callback).not.toHaveBeenCalled();
  });

  it('rejects malformed receipt facts before invoking callbacks, retaining valid omitted facts exactly', async () => {
    const callback = vi.fn(async () => ({ signature: hex }));
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: callback });
    for (const receipt of [
      { stampMs: -1 }, { expiryMs: -1 }, { stampMs: 1.5 }, { expiryMs: 2.5 },
      { stampMs: Number.MAX_SAFE_INTEGER + 1 }, { expiryMs: Number.MAX_SAFE_INTEGER + 1 },
      { stampMs: Infinity }, { expiryMs: NaN }, { stampMs: 5, expiryMs: 5 }, { stampMs: 6, expiryMs: 5 },
      { stampMs: null }, { arbitrary: 1 }, null,
    ]) {
      await expect(signer({ ...request(), receipt } as SignRequest, context())).rejects.toThrow('Remote signing request refused');
    }
    expect(callback).not.toHaveBeenCalled();
    for (const receipt of [{}, { stampMs: 0 }, { expiryMs: Number.MAX_SAFE_INTEGER }, { stampMs: 0, expiryMs: 1 }]) {
      expect((await signer({ ...request(), receipt }, context())).receipt).toEqual(receipt);
    }
  });

  it('matches the shared nonempty 256-byte opaque reference grammar without normalizing refs', async () => {
    const callback = vi.fn(async () => ({ signature: hex }));
    for (const key of ['', 'é'.repeat(129)]) {
      for (const field of ['credential', 'signing']) {
        expect(() => createRemoteSignerProvider({ exchangeName: 'binance',
          identities: { primary: { ...primary, [field]: key } }, payloadSigner: callback }))
          .toThrow('Remote signing request refused');
      }
    }
    for (const signing of ['é'.repeat(128), '   ', 'ref\0suffix']) {
      const slot = { ...primary, signing };
      const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities: { primary: slot }, payloadSigner: callback });
      expect((await signer(request(slot), context())).values).toHaveLength(2);
    }
    expect(callback).toHaveBeenCalledTimes(3);
  });

  it('bounds aggregate payload across multiple operations, including repeated public credentials', async () => {
    const callback = vi.fn(async () => ({ signature: hex }));
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: callback });
    const operations: SignRequest['operations'] = Array.from({ length: 2 }, () => ({
      kind: 'sign', key: primary.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: Array(600_000).fill(65) }],
    }));
    await expect(signer({ ...request(), operations }, context())).rejects.toThrow('Remote signing request refused');
    expect(callback).not.toHaveBeenCalled();
  });

  it.each([
    { Bad: 'line\r\nbreak' }, { Bad: 'null\0' }, { 'bad name': 'x' }, { Bad: 'x'.repeat(8193) },
    Object.fromEntries(Array.from({ length: 33 }, (_, i) => [`X-${i}`, 'x'])),
    Object.fromEntries(Array.from({ length: 5 }, (_, i) => [`X-${i}`, 'é'.repeat(4096)])),
    { ['X'.repeat(257)]: 'x' },
  ])('rejects malformed or oversized callback headers atomically', async (headers) => {
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: () => ({ signature: hex, headers }) });
    await expect(signer(request(), context())).rejects.toThrow('Remote signing request refused');
  });

  it('refuses headers on ambiguous multiple-HMAC batches but allows headerless multiple results', async () => {
    const callback = vi.fn(async () => ({ signature: hex, headers: { X: 'one' } }));
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities, payloadSigner: callback });
    const input = request();
    input.operations.push(request().operations[1]);
    await expect(signer(input, context())).rejects.toThrow('Remote signing request refused');
    expect(callback).toHaveBeenCalledTimes(1);
    callback.mockResolvedValue({ signature: hex } as never);
    const result: SignResult = await signer(input, context());
    expect(result.values).toHaveLength(3);
    expect(result).not.toHaveProperty('headers');
  });

  it('snapshots trusted public material and receipt against mutation while callbacks wait', async () => {
    const slot = { ...primary };
    const pending = deferred<{ signature: string }>();
    const callback: PayloadSigner = () => pending.promise;
    const signer = createRemoteSignerProvider({ exchangeName: 'binance', identities: { primary: slot }, payloadSigner: callback });
    slot.publicCredential = 'changed';
    const input = request();
    const result = signer(input, context());
    input.receipt.stampMs = 1;
    pending.resolve({ signature: hex });
    expect((await result).receipt.stampMs).toBe(1700000000123);
  });

  it('rejects missing callbacks and ambiguous/invalid identity setup with closed errors', () => {
    expect(() => createRemoteSignerProvider({ exchangeName: 'binance', identities })).toThrow('Remote signing request refused');
    for (const invalid of [
      { ...primary, signing: primary.credential },
      { ...secondary, credential: primary.credential },
      { ...secondary, publicCredential: '\ud800' },
    ]) {
      expect(() => createRemoteSignerProvider({ exchangeName: 'binance', identities: { primary, secondary: invalid }, payloadSigner: vi.fn() }))
        .toThrow('Remote signing request refused');
    }
  });
});

describe('Bitunix fused double-SHA256 remote admission', () => {
  const material = 'test-secret-value';
  const own = { credential: 'owned/bitunix/public', signing: 'owned/bitunix/signing', publicCredential: 'api-key-xyz' };
  const read = { credential: 'owned/bitunix/read', signing: 'owned/bitunix/read-signing', publicCredential: 'read-key' };
  const slots = { primary: own, secondary: read };
  const bytes = (text: string) => Array.from(Buffer.from(text, 'utf8'));
  // Independent Node crypto oracle of the browser's `bitunix-double-sha256`.
  const doubleSha256 = (toSign: string) => createHash('sha256')
    .update(createHash('sha256').update(toSign).digest('hex') + material).digest('hex');
  const fused = (signed: string, slot = own): SignRequest => ({
    receipt: { stampMs: 1700000000000 },
    context: { venue: 'untrusted', method: 'POST', host: 'untrusted.invalid', path: '/not-authority' },
    operations: [
      { kind: 'sha256', payload: [{ kind: 'bytes', bytes: bytes('nonce-abc1700000000') },
        { kind: 'credential', key: slot.credential }, { kind: 'bytes', bytes: bytes(signed) }] },
      { kind: 'sign', algorithm: 'sha256SecretSuffix', key: slot.signing, payload: [{ kind: 'result', index: 0, encoding: 'hex' }] },
      { kind: 'present', key: slot.credential },
    ],
  });
  const browser = (answer?: (toSign: string) => unknown) =>
    remote(async ({ id, wsAuth }) => ({ id, wsAuthSignature: (answer ?? doubleSha256)(wsAuth!.toSign) as string }));

  it.each([
    // Python hashlib vectors: the TS REST preimage of a signed POST, and the login.
    ['rest', 'symbolBTCUSDT{"qty":"1"}', '7ddef339d8ec2d6edade44ed8013ff647e2ae9b7b51926b438342c9dfe3721d3'],
    // Node crypto vector from base.ws-auth-signing.test.ts.
    ['login', '', 'c99d3e027fcb8cbefc05f7a38dfc710011d07e79dbefd8b0bc7b44347bb0e624'],
  ])('signs the %s preimage through the legacy wire and returns the local-key values', async (_, signed, golden) => {
    const r = browser();
    const fallback = vi.fn();
    const signer = createRemoteSignerProvider({ exchangeName: 'bitunix', identities: slots, remoteSigner: r.value, payloadSigner: fallback });
    const input = fused(signed);
    const output = await signer(input, context());
    const toSign = `nonce-abc1700000000api-key-xyz${signed}`;
    if (signed) {
      const config = { url: '/api/v1/futures/trade/place_order', params: { symbol: 'BTCUSDT' }, data: { qty: '1' }, headers: {} };
      expect(prepareBitunixRequest(config as never, 'api-key-xyz', 1700000000, 'nonce-abc').payload).toBe(toSign);
    }
    // One envelope, no extraData: the browser ignores it and HTTP shares the login shape.
    expect(r.value.sign).toHaveBeenCalledOnce();
    expect(r.value.sign).toHaveBeenCalledWith({ id: expect.any(String), type: 'ws-auth', exchangeName: 'bitunix',
      accountName: 'account-one', wsAuth: { toSign, outputFormat: 'bitunix-double-sha256' } });
    expect(Buffer.from(output.values[1]).toString('hex')).toBe(golden);
    expect(output).toEqual({ receipt: input.receipt, values: [
      Array.from(createHash('sha256').update(toSign).digest()), Array.from(Buffer.from(golden, 'hex')), bytes('api-key-xyz')] });
    expect(fallback).not.toHaveBeenCalled();
  });

  it('accepts an uppercase 64-hex answer as the same 32 bytes', async () => {
    const r = browser((toSign) => doubleSha256(toSign).toUpperCase());
    const output = await createRemoteSignerProvider({ exchangeName: 'bitunix', identities: slots, remoteSigner: r.value })(fused(''), context());
    expect(Buffer.from(output.values[1]).toString('hex')).toBe('c99d3e027fcb8cbefc05f7a38dfc710011d07e79dbefd8b0bc7b44347bb0e624');
  });

  const mutations: [string, (operations: SignRequest['operations']) => SignRequest['operations']][] = [
    ['foreign credential part', ([, sign, present]) => [{ kind: 'sha256', payload: [
      { kind: 'bytes', bytes: bytes('nonce-abc1700000000') }, { kind: 'credential', key: read.credential }] }, sign, present]],
    ['unowned credential part', ([, sign, present]) => [{ kind: 'sha256', payload: [{ kind: 'credential', key: 'another-account/public' }] }, sign, present]],
    ['present first', ([digest, sign, present]) => [present, digest, sign]],
    ['sign before sha256', ([digest, sign, present]) => [sign, digest, present]],
    ['duplicated sha256', ([digest, sign, present]) => [digest, digest, sign, present]],
    ['duplicated sign', ([digest, sign, present]) => [digest, sign, sign, present]],
    ['extra present', ([digest, sign, present]) => [digest, sign, present, present]],
    ['missing present', ([digest, sign]) => [digest, sign]],
    ['missing sha256', ([, sign, present]) => [sign, present]],
    ['foreign present', ([digest, sign]) => [digest, sign, { kind: 'present', key: read.credential }]],
    ['secondary signing key', ([digest, , present]) => [digest,
      { kind: 'sign', algorithm: 'sha256SecretSuffix', key: read.signing, payload: [{ kind: 'result', index: 0, encoding: 'hex' }] }, present]],
    ['hmac over the digest', ([digest, , present]) => [digest,
      { kind: 'sign', algorithm: 'hmacSha256', key: own.signing, payload: [{ kind: 'result', index: 0, encoding: 'hex' }] }, present]],
    ['wrong result index', ([digest, , present]) => [digest,
      { kind: 'sign', algorithm: 'sha256SecretSuffix', key: own.signing, payload: [{ kind: 'result', index: 1, encoding: 'hex' }] }, present]],
    ['raw result encoding', ([digest, , present]) => [digest,
      { kind: 'sign', algorithm: 'sha256SecretSuffix', key: own.signing, payload: [{ kind: 'result', index: 0, encoding: 'raw' }] }, present]],
    ['extra sign part', ([digest, , present]) => [digest,
      { kind: 'sign', algorithm: 'sha256SecretSuffix', key: own.signing, payload: [{ kind: 'result', index: 0, encoding: 'hex' }, { kind: 'bytes', bytes: [65] }] }, present]],
    ['bytes in place of the result', ([digest, , present]) => [digest,
      { kind: 'sign', algorithm: 'sha256SecretSuffix', key: own.signing, payload: [{ kind: 'bytes', bytes: [65] }] }, present]],
    ['result inside the digest', ([, sign, present]) => [{ kind: 'sha256', payload: [{ kind: 'result', index: 0, encoding: 'hex' }] }, sign, present]],
    ['empty digest', ([, sign, present]) => [{ kind: 'sha256', payload: [] }, sign, present]],
    ['invalid byte', ([, sign, present]) => [{ kind: 'sha256', payload: [{ kind: 'bytes', bytes: [256] }] }, sign, present]],
    ['split UTF-8', ([, sign, present]) => [{ kind: 'sha256', payload: [{ kind: 'bytes', bytes: [0xc3] }] }, sign, present]],
    ['lone suffix sign', () => [{ kind: 'sign', algorithm: 'sha256SecretSuffix', key: own.signing, payload: [{ kind: 'bytes', bytes: [65] }] }]],
    ['lone sha256', ([digest]) => [digest]],
  ];
  it.each(mutations)('refuses %s before any callback', async (_, mutate) => {
    const r = browser();
    const signer = createRemoteSignerProvider({ exchangeName: 'bitunix', identities: slots, remoteSigner: r.value });
    const input = fused('');
    await expect(signer({ ...input, operations: mutate(input.operations) }, context())).rejects.toThrow(/^Remote signing request refused$/);
    expect(r.value.sign).not.toHaveBeenCalled();
  });

  it.each([
    ['non-hex', () => 'z'.repeat(64)], ['short', (t: string) => doubleSha256(t).slice(2)], ['long', (t: string) => `${doubleSha256(t)}00`],
    ['prefixed', (t: string) => `0x${doubleSha256(t).slice(2)}`], ['padded', (t: string) => ` ${doubleSha256(t)}`],
    ['base64', () => 'SL/xd+lRj5sirx9sxjcCAiy6l78Q3WwR8844XzqxcNs='], ['number', () => 1], ['absent', () => undefined],
  ])('refuses a %s browser answer without publishing any value', async (_, answer) => {
    const r = browser(answer as (toSign: string) => unknown);
    const signer = createRemoteSignerProvider({ exchangeName: 'bitunix', identities: slots, remoteSigner: r.value });
    await expect(signer(fused(''), context())).rejects.toThrow(/^Remote signing request refused$/);
    expect(r.value.sign).toHaveBeenCalledOnce();
  });

  it('refuses the fused shape for PayloadSigner, for other venues and for HMAC on Bitunix', async () => {
    const callback = vi.fn(async () => ({ signature: hex }));
    const payload = createRemoteSignerProvider({ exchangeName: 'bitunix', identities: slots, payloadSigner: callback });
    await expect(payload(fused(''), context())).rejects.toThrow(/^Remote signing request refused$/);
    expect(callback).not.toHaveBeenCalled();
    for (const exchangeName of ['bybit', 'binance', 'okx', 'bitget'] as const) {
      const r = browser();
      const signer = createRemoteSignerProvider({ exchangeName, identities: slots, remoteSigner: r.value });
      await expect(signer(fused(''), context())).rejects.toThrow(/^Remote signing request refused$/);
      expect(r.value.sign).not.toHaveBeenCalled();
    }
    const r = browser();
    const bitunix = createRemoteSignerProvider({ exchangeName: 'bitunix', identities, remoteSigner: r.value });
    await expect(bitunix(request(), context())).rejects.toThrow(/^Remote signing request refused$/);
    expect(r.value.sign).not.toHaveBeenCalled();
  });

  it('advertises sha256SecretSuffix only for Bitunix', () => {
    expect(remoteSignerAlgorithms('bitunix')).toEqual(['sha256SecretSuffix']);
    for (const name of ['bybit', 'bybitinverse', 'binance', 'binancespot', 'okx', 'okxeu', 'bitget', 'bitgetusdc', 'hyrotrader'] as const) {
      expect(remoteSignerAlgorithms(name), name).toEqual(['hmacSha256']);
    }
  });
});

describe('BloFin base64-of-hex remote admission', () => {
  const material = 'test-secret-value';
  const own = { credential: 'owned/blofin/public', signing: 'owned/blofin/signing', publicCredential: 'blofin-key',
    passphrase: 'owned/blofin/passphrase', passphraseValue: 'blofin-pass' };
  const bytes = (text: string) => Array.from(Buffer.from(text, 'utf8'));
  // Independent Node crypto oracle of BloFin's ACCESS-SIGN and login sign.
  const blofinMac = (toSign: string) =>
    Buffer.from(createHmac('sha256', material).update(toSign).digest('hex')).toString('base64');
  const parts = ['/api/v1/account/balance', 'GET', '1700000000000', 'nonce-1', ''];
  const work = (): SignRequest => ({
    receipt: { stampMs: 1700000000000 },
    context: { venue: 'untrusted', method: 'GET', host: 'untrusted.invalid', path: '/not-authority' },
    operations: [
      { kind: 'present', key: own.credential },
      { kind: 'sign', algorithm: 'hmacSha256', key: own.signing, payload: parts.map((part) => ({ kind: 'bytes' as const, bytes: bytes(part) })) },
      { kind: 'present', key: own.passphrase },
    ],
  });
  const browser = (answer?: (toSign: string) => unknown) =>
    remote(async ({ id, wsAuth }) => ({ id, wsAuthSignature: (answer ?? blofinMac)(wsAuth!.toSign) as string }));

  it('signs the exact preimage through the hex-base64 wire and returns the raw MAC and presentations', async () => {
    const r = browser();
    const signer = createRemoteSignerProvider({ exchangeName: 'blofin', identities: { primary: own }, remoteSigner: r.value });
    const input = work();
    const output = await signer(input, context());
    const toSign = parts.join('');
    expect(r.value.sign).toHaveBeenCalledWith({ id: expect.any(String), type: 'ws-auth', exchangeName: 'blofin',
      accountName: 'account-one', wsAuth: { toSign, outputFormat: 'hex-base64' } });
    expect(output).toEqual({ receipt: input.receipt, values: [
      bytes('blofin-key'), Array.from(createHmac('sha256', material).update(toSign).digest()), bytes('blofin-pass')] });
  });

  it('refuses any answer that is not the canonical base64 of 64 lowercase hex characters', async () => {
    const hexText = (toSign: string) => createHmac('sha256', material).update(toSign).digest('hex');
    for (const answer of [
      hexText,
      (toSign: string) => createHmac('sha256', material).update(toSign).digest('base64'),
      (toSign: string) => Buffer.from(hexText(toSign).toUpperCase()).toString('base64'),
      (toSign: string) => Buffer.from(hexText(toSign)).toString('base64').replace(/==$/, ''),
      (toSign: string) => Buffer.from(hexText(toSign).slice(2)).toString('base64'),
    ]) {
      const signer = createRemoteSignerProvider({ exchangeName: 'blofin', identities: { primary: own }, remoteSigner: browser(answer).value });
      await expect(signer(work(), context())).rejects.toThrow(/^Remote signing request refused$/);
    }
  });

  it('refuses a BloFin PayloadSigner at setup', () => {
    expect(() => createRemoteSignerProvider({ exchangeName: 'blofin', identities: { primary: own }, payloadSigner: vi.fn() }))
      .toThrow(/^Remote signing request refused$/);
  });
});
