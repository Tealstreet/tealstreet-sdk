import type { SignRequest, SignResult } from '@tealstreet/safe-cex-wasm/host';
import type { PayloadSigner, RemoteSigner } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';

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
