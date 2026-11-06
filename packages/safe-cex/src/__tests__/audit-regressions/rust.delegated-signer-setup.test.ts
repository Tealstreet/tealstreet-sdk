import type { SignRequest, SigningIdentities, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, RemoteSigner } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { derivePublicMode } from '../../exchanges/rust/construction';

const hex = 'cd'.repeat(32);
const payloadSigner = () => vi.fn(async () => ({ signature: hex, headers: { 'X-Callback': 'owned' } }));
const options = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: 'public-primary', secret: 'audit-secret-sentinel', passphrase: 'audit-secret-sentinel',
  secondary_key: 'public-read', secondary_secret: 'audit-secret-sentinel', secondary_passphrase: 'audit-secret-sentinel',
  payloadSigner: payloadSigner(), ...extra,
});
const request = (identities: SigningIdentities, secondary = false): SignRequest => {
  const identity = secondary ? identities.secondary! : identities.primary!;
  return { receipt: { stampMs: 10 }, context: { venue: 'advisory', method: 'GET', host: 'invalid', path: '/' }, operations: [
    { kind: 'present', key: identity.credential },
    { kind: 'sign', key: identity.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: [65] }, { kind: 'credential', key: identity.credential }] },
  ] };
};
const callContext = () => ({ signal: new AbortController().signal });
function construct(setup: DelegatedSignerSetup, venue: ExchangeName = 'binance', json = '{}') {
  const calls: unknown[][] = [];
  class Engine { constructor(...args: unknown[]) { calls.push(args); } }
  const result = setup.construct(Engine as unknown as new (name: string, options: string) => WasmEngineLike, venue, json);
  return { result, args: calls[0], config: JSON.parse(calls[0][1] as string) as { signing: SigningIdentities } };
}
const remote = (): RemoteSigner => ({
  accountName: 'owned-account', sign: vi.fn(async ({ id }) => ({ id, wsAuthSignature: hex })),
  isAvailable: () => true, onAvailabilityChange: () => () => {},
});
const expectInvalid = (invoke: () => unknown) => {
  let error: unknown;
  try { invoke(); } catch (caught) { error = caught; }
  expect(error).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
};

describe('trusted delegated signer setup capsule', () => {
  it.each(['binance', 'binancepm', 'binanceinverse', 'binancespot', 'bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'] as const)(
    '%s constructs through opaque refs with exactly two arguments, no local setup blob or key material', async (venue) => {
      const input = options();
      const setup = new DelegatedSignerSetup(venue, input);
      const { args, config } = construct(setup, venue, JSON.stringify({ ...input, testnet: true, readCredentials: { key: 'audit-secret-sentinel', secret: 'audit-secret-sentinel', passphrase: 'audit-secret-sentinel' }, signing: { primary: { signing: 'foreign' } } }));
      expect(args).toHaveLength(2);
      expect(args[0]).toBe(venue);
      expect(config).toMatchObject({ testnet: true, signing: { primary: { credential: expect.any(String), signing: expect.any(String) }, secondary: { credential: expect.any(String), signing: expect.any(String) } } });
      for (const text of [String(args[1]), JSON.stringify(setup)]) {
        expect(text).not.toContain('audit-secret-sentinel');
        expect(text).not.toContain('public-primary');
        expect(text).not.toContain('public-read');
      }
      for (const field of ['key', 'secret', 'passphrase', 'secondary_key', 'secondary_secret', 'secondary_passphrase', 'remoteSigner', 'localSigner', 'readCredentials']) expect(config).not.toHaveProperty(field);
      expect(config).toMatchObject({ remoteSigning: true, payloadSigner: true, publicMode: { payloadSigner: true, remoteSigner: false } });
      expect(config.signing.primary).not.toHaveProperty('stableId');
      expect(setup.primaryComplete).toBe(true);
      expect(setup.hasSecondary).toBe(true);
      expect(setup.credentialPresence).toEqual({ key: true, secret: true });
      const signed = await setup.signer(request(config.signing), callContext());
      expect(signed.values[0]).toEqual(Array.from(new TextEncoder().encode('public-primary')));
      expect(input.key).toBe('public-primary');
      expect(input.secret).toBe('audit-secret-sentinel');
      expectInvalid(() => construct(setup, venue));
      setup.dispose();
    },
  );

  it('advertises per-venue signer algorithms: sha256SecretSuffix for Bitunix only', () => {
    expect(new DelegatedSignerSetup('bitunix', options({ remoteSigner: remote() })).signerAlgorithms).toEqual(['sha256SecretSuffix']);
    for (const venue of ['bybit', 'binance', 'okx', 'bitget'] as ExchangeName[]) {
      expect(new DelegatedSignerSetup(venue, options({ remoteSigner: remote() })).signerAlgorithms, venue).toEqual(['hmacSha256']);
    }
  });

  it('allocates independent opaque references per setup without using key fingerprints or inventing stable IDs', () => {
    const input = options({ localSigner: { primary: { stableId: 'caller/account' }, secondary: { stableId: 'caller/read' } } });
    const first = new DelegatedSignerSetup('binance', input);
    const second = new DelegatedSignerSetup('binance', input);
    const a = construct(first).config.signing;
    const b = construct(second).config.signing;
    const refs = [a.primary!.credential, a.primary!.signing, a.secondary!.credential, a.secondary!.signing,
      b.primary!.credential, b.primary!.signing, b.secondary!.credential, b.secondary!.signing];
    expect(new Set(refs).size).toBe(8);
    expect(refs.every((ref) => !ref.includes('public-'))).toBe(true);
    expect(a.primary!.stableId).toBe('caller/account');
    expect(a.secondary!.stableId).toBe('caller/read');
    first.dispose(); second.dispose();
  });

  it('snapshots selected public credentials and callbacks before caller input is stripped or changed', async () => {
    const callback = payloadSigner();
    const input = options({ payloadSigner: callback });
    const setup = new DelegatedSignerSetup('binance', input);
    Object.assign(input, { key: 'changed', secondary_key: 'changed-read', payloadSigner: vi.fn(() => { throw new Error('wrong callback'); }) });
    const { config } = construct(setup);
    await setup.signer(request(config.signing, true), callContext());
    expect(callback).toHaveBeenCalledWith({ exchangeName: 'binance', key: 'public-read', payload: 'Apublic-read', scheme: 'hmac-sha256', encoding: 'hex' });
    setup.dispose();
  });

  it('binds PayloadSigner to an immutable private public-metadata snapshot for the selected slot', async () => {
    const receivers: Record<string, unknown>[] = [];
    const callback = function (this: Record<string, unknown>, work: { key: string }) {
      receivers.push(this);
      expect(this.key).toBe(work.key);
      return { signature: hex };
    };
    const input = options({ payloadSigner: callback, testnet: true, subaccount: 'owned-subaccount', extra: { recvWindow: 7000 } });
    const setup = new DelegatedSignerSetup('binance', input);
    const { config } = construct(setup);
    Object.assign(input, { key: 'changed', testnet: false, subaccount: 'changed' });
    input.extra!.recvWindow = 1;
    await setup.signer(request(config.signing), callContext());
    await setup.signer(request(config.signing, true), callContext());
    expect(receivers.map((receiver) => receiver.key)).toEqual(['public-primary', 'public-read']);
    for (const receiver of receivers) {
      expect(receiver).not.toBe(input);
      expect(Object.isFrozen(receiver)).toBe(true);
      expect(Object.isFrozen(receiver.extra)).toBe(true);
      expect(receiver).toMatchObject({ testnet: true, subaccount: 'owned-subaccount', extra: { recvWindow: 7000 } });
      for (const field of ['secret', 'passphrase', 'secondary_secret', 'secondary_passphrase', 'auth', 'localSigner', 'remoteSigner', 'payloadSigner']) expect(receiver).not.toHaveProperty(field);
      expect(JSON.stringify(receiver)).not.toContain('audit-secret-sentinel');
    }
    setup.dispose();
  });

  it('keeps captured callback, selected feed mode and raw intent despite mutated options or forged engine JSON', () => {
    const input = options({ remoteSigner: remote(), publicDataOnly: false });
    const setup = new DelegatedSignerSetup('binance', input);
    Object.assign(input, { remoteSigner: undefined, payloadSigner: undefined, publicDataOnly: true });
    const { config } = construct(setup, 'binance', JSON.stringify({ payloadSigner: false, remoteSigning: false,
      publicMode: { requested: true, payloadSigner: false, remoteSigner: false } }));
    expect(setup.intent).toEqual({ remoteSigner: true, payloadSigner: true });
    expect(config).toMatchObject({ remoteSigning: true, payloadSigner: false,
      publicMode: { requested: false, payloadSigner: true, remoteSigner: true } });
    setup.dispose();
  });

  it('never falls back when a configured callback is malformed, including null or a lower-precedence callback', () => {
    for (const callbacks of [
      { remoteSigner: null, payloadSigner: payloadSigner() },
      { remoteSigner: {}, payloadSigner: payloadSigner() },
      { remoteSigner: remote(), payloadSigner: null },
      { payloadSigner: 'not-callable' },
    ]) expectInvalid(() => new DelegatedSignerSetup('binance', { ...options(), ...callbacks } as unknown as ExchangeOptions));
  });

  it('keeps RemoteSigner authority ahead of PayloadSigner and local secrets; refuses actual secondary signing only at the provider', async () => {
    const upstream = remote();
    const fallback = payloadSigner();
    const setup = new DelegatedSignerSetup('binance', options({ remoteSigner: upstream, payloadSigner: fallback, secondary_secret: undefined }));
    const { config } = construct(setup);
    expect(setup.hasSecondary).toBe(true);
    await setup.signer(request(config.signing), callContext());
    expect(upstream.sign).toHaveBeenCalledTimes(1);
    await expect(setup.signer(request(config.signing, true), callContext())).rejects.toThrow('Remote signing request refused');
    expect(upstream.sign).toHaveBeenCalledTimes(1);
    expect(fallback).not.toHaveBeenCalled();
    setup.dispose();
  });

  it('admits delegated secondary public-key selection without a local secondary secret', async () => {
    const callback = payloadSigner();
    const setup = new DelegatedSignerSetup('bybit', options({ payloadSigner: callback, secondary_secret: undefined, secondary_passphrase: undefined }));
    const { config } = construct(setup, 'bybit');
    await setup.signer(request(config.signing, true), callContext());
    expect(callback).toHaveBeenCalledWith({ exchangeName: 'bybit', key: 'public-read', payload: 'Apublic-read', scheme: 'hmac-sha256', encoding: 'hex' });
    setup.dispose();
  });

  it.each([undefined, ''])('preserves public-only primary bypass for key=%s without manufacturing refs or signing authority', async (key) => {
    const callback = payloadSigner();
    const input = options({ key, payloadSigner: callback, secondary_key: undefined, secondary_secret: undefined, secondary_passphrase: undefined });
    const setup = new DelegatedSignerSetup('binance', input);
    expect(setup.primaryComplete).toBe(false);
    expect(setup.credentialPresence).toEqual({ key: false, secret: true });
    expect(derivePublicMode('binance', input, undefined, setup.credentialPresence).publicDataOnly).toBe(true);
    const { config } = construct(setup);
    expect(config.signing).toEqual({});
    const foreign = { primary: { credential: 'unowned', signing: 'unowned-signing' } };
    await expect(setup.signer(request(foreign), callContext())).rejects.toThrow('Remote signing request refused');
    expect(callback).not.toHaveBeenCalled();
    setup.dispose();
  });

  it('honors explicit Rust White_Space trimming while preserving exact public key bytes by default', async () => {
    for (const [key, trimCredential, expected, complete] of [
      ['\u0085 key \u0085', undefined, '\u0085 key \u0085', true],
      ['\u0085 key \u0085', true, 'key', true],
      ['\ufeff', true, '\ufeff', true],
      ['\u0085', true, undefined, false],
    ] as const) {
      const callback = payloadSigner();
      const setup = new DelegatedSignerSetup('binance', options({ key, payloadSigner: callback,
        localSigner: { primary: { trimCredential } }, secondary_key: undefined, secondary_secret: undefined, secondary_passphrase: undefined }));
      expect(setup.primaryComplete).toBe(complete);
      const { config } = construct(setup);
      if (complete) {
        await setup.signer(request(config.signing), callContext());
        expect(callback).toHaveBeenCalledWith({ exchangeName: 'binance', key: expected, payload: `A${expected}`, scheme: 'hmac-sha256', encoding: 'hex' });
      } else expect(config.signing.primary).toBeUndefined();
      setup.dispose();
    }
  });

  it('disposal cancels in-flight work, drops authority and makes retained host signer wrappers inert', async () => {
    let reply!: (value: { signature: string }) => void;
    const callback = vi.fn(() => new Promise<{ signature: string }>((resolve) => { reply = resolve; }));
    const setup = new DelegatedSignerSetup('binance', options({ payloadSigner: callback }));
    const { config } = construct(setup);
    const copied = setup.signer;
    const inFlight = copied(request(config.signing), callContext());
    const canceled = expect(inFlight).rejects.toThrow('Remote signing canceled');
    setup.dispose();
    await canceled;
    await expect(copied(request(config.signing), callContext())).rejects.toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
    reply({ signature: hex });
    await Promise.resolve();
    expect(callback).toHaveBeenCalledTimes(1);
    expectInvalid(() => construct(setup));
    setup.dispose();
  });

  it('propagates external abort and releases its listener without affecting other attempts', async () => {
    const callback = payloadSigner();
    const setup = new DelegatedSignerSetup('binance', options({ payloadSigner: callback }));
    const { config } = construct(setup);
    const controller = new AbortController();
    const remove = vi.spyOn(controller.signal, 'removeEventListener');
    controller.abort();
    await expect(setup.signer(request(config.signing), { signal: controller.signal })).rejects.toMatchObject({ outcome: 'notSent' });
    expect(callback).not.toHaveBeenCalled();
    const active = new AbortController();
    const cleanup = vi.spyOn(active.signal, 'removeEventListener');
    await setup.signer(request(config.signing), { signal: active.signal });
    expect(cleanup).toHaveBeenCalledWith('abort', expect.any(Function));
    expect(remove).not.toHaveBeenCalled();
    setup.dispose();
  });

  it('forwards an active external abort without disposing its account provider', async () => {
    const callback = vi.fn(() => new Promise<{ signature: string }>(() => {}));
    const setup = new DelegatedSignerSetup('binance', options({ payloadSigner: callback }));
    const { config } = construct(setup);
    const controller = new AbortController();
    const remove = vi.spyOn(controller.signal, 'removeEventListener');
    const active = setup.signer(request(config.signing), { signal: controller.signal });
    const canceled = expect(active).rejects.toThrow('Remote signing canceled');
    controller.abort();
    await canceled;
    expect(remove).toHaveBeenCalledWith('abort', expect.any(Function));
    callback.mockResolvedValue({ signature: hex });
    expect((await setup.signer(request(config.signing), callContext())).values).toHaveLength(2);
    expect(callback).toHaveBeenCalledTimes(2);
    setup.dispose();
  });

  it('bounds pending work, then disposal settles every admitted attempt without invoking another callback', async () => {
    const callback = vi.fn(() => new Promise<{ signature: string }>(() => {}));
    const setup = new DelegatedSignerSetup('binance', options({ payloadSigner: callback }));
    const { config } = construct(setup);
    const pending = Array.from({ length: 64 }, () => setup.signer(request(config.signing), callContext()));
    await expect(setup.signer(request(config.signing), callContext())).rejects.toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
    expect(callback).toHaveBeenCalledTimes(64);
    const settled = Promise.allSettled(pending);
    setup.dispose();
    const results = await settled;
    expect(results.every((result) => result.status === 'rejected' && result.reason.message === 'Remote signing canceled')).toBe(true);
    expect(callback).toHaveBeenCalledTimes(64);
  });

  it('consumes construction exactly once on native failure, malformed options or wrong venue', async () => {
    for (const kind of ['native', 'json', 'venue'] as const) {
      const callback = payloadSigner();
      const setup = new DelegatedSignerSetup('binance', options({ payloadSigner: callback }));
      class Engine { constructor() { throw new Error('native construction refused'); } }
      if (kind === 'native') expect(() => setup.construct(Engine as never, 'binance', '{}')).toThrow('native construction refused');
      else expectInvalid(() => setup.construct(Engine as never, kind === 'venue' ? 'bybit' : 'binance', kind === 'json' ? '[' : '{}'));
      expectInvalid(() => construct(setup));
      await expect(setup.signer({} as SignRequest, callContext())).rejects.toMatchObject({ outcome: 'notSent' });
      expect(callback).not.toHaveBeenCalled();
    }
  });

  it('rejects incomplete/malformed delegated input and reuses the common metadata validator with typed failures', () => {
    for (const extra of [
      { payloadSigner: undefined }, { payloadSigner: 'not-a-function' }, { remoteSigner: {} },
      { key: 7 }, { key: 'line\nbreak' }, { secondary_key: undefined }, { secondary_key: '' },
      { localSigner: null }, { localSigner: { primary: { stableId: '' } } },
      { localSigner: { primary: { secretEncoding: 'invalid' } } }, { localSigner: { tertiary: {} } },
      { localSigner: { secondary: { trimCredential: true } }, secondary_key: ' \t' },
    ]) expectInvalid(() => new DelegatedSignerSetup('binance', options(extra as Partial<ExchangeOptions>)));
    // A present local secret alone never enables this callback-only capsule.
    expectInvalid(() => new DelegatedSignerSetup('binance', options({ remoteSigner: undefined, payloadSigner: undefined })));
  });
});
