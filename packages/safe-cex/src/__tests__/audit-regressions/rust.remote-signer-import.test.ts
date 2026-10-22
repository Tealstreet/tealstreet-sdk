import { expect, it, vi } from 'vitest';

it('imports the delegated provider before native codecs exist and signs after host installation', async () => {
  const Encoder = globalThis.TextEncoder;
  const Decoder = globalThis.TextDecoder;
  vi.resetModules();
  vi.stubGlobal('TextEncoder', undefined);
  vi.stubGlobal('TextDecoder', undefined);
  let imported: typeof import('../../exchanges/rust/remoteSignerProvider');
  try {
    imported = await import('../../exchanges/rust/remoteSignerProvider');
  } finally {
    vi.stubGlobal('TextEncoder', Encoder);
    vi.stubGlobal('TextDecoder', Decoder);
    vi.unstubAllGlobals();
  }
  const sign = vi.fn(async () => ({ signature: 'ab'.repeat(32) }));
  const provider = imported.createRemoteSignerProvider({
    exchangeName: 'binance',
    identities: { primary: { credential: 'ref:owned-primary', signing: 'ref:owned-signing', publicCredential: 'public+é' } },
    payloadSigner: sign,
  });
  const result = await provider({
    receipt: {}, context: { venue: '', method: '', host: '', path: '' },
    operations: [
      { kind: 'present', key: 'ref:owned-primary' },
      { kind: 'sign', key: 'ref:owned-signing', algorithm: 'hmacSha256', payload: [
        { kind: 'bytes', bytes: Array.from(new Encoder().encode('exact-é')) },
      ] },
    ],
  }, { signal: new AbortController().signal });
  expect(sign).toHaveBeenCalledExactlyOnceWith({
    exchangeName: 'binance', key: 'public+é', payload: 'exact-é', scheme: 'hmac-sha256', encoding: 'hex',
  });
  expect(result.values).toEqual([Array.from(new Encoder().encode('public+é')), Array(32).fill(0xab)]);
});
