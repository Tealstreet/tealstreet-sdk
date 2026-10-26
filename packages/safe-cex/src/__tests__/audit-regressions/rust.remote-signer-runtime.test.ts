import type { Signer, SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions, RemoteSigner } from '../../types';

import { createHash, createHmac } from 'node:crypto';

import { afterEach, describe, expect, it, vi } from 'vitest';


import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import {
  runsOnRustEngine,
  RustEngineExchange,
  setRustEngineTransport,
  setWasmEngineLoader,
} from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { engineCapabilities } from '../fixtures/engineCapabilities';

const aliases = ['binance', 'binancepm', 'binanceinverse', 'binancespot'] as const;
const running: RustEngineExchange[] = [];
const mac = (text: string) => createHmac('sha256', 'audit-provider').update(text).digest('hex');
const remote = (): RemoteSigner => ({
  accountName: 'owned-account',
  sign: vi.fn(async (request) => ({ id: request.id, wsAuthSignature: mac(request.wsAuth!.toSign) })),
  isAvailable: () => true,
  onAvailabilityChange: () => () => {},
});
const payload = () =>
  vi.fn(async ({ payload }: { payload: string }) => ({ signature: mac(payload), headers: { 'X-Callback': 'owned' } }));
const setup = (name: (typeof aliases)[number] | 'bitunix', input: Partial<ExchangeOptions> = {}, failCapabilities = false) => {
  const calls: unknown[][] = [];
  const installed = vi.fn();
  const signed = vi.fn((_attempt: number, _result: string, _now: number) => '[]');
  const failed = vi.fn((_attempt: number, _code: string, _now: number) => '[]');
  const free = vi.fn();
  class Engine {
    constructor(...args: unknown[]) {
      calls.push(args);
    }
    capabilities() {
      if (failCapabilities) throw new Error('capability initialization failed');
      return engineCapabilities(name);
    }
    requiresExternalSigner() {
      return true;
    }
    useExternalSigner = installed;
    signed = signed;
    signFailed = failed;
    free = free;
  }
  setWasmEngineLoader(async () => ({ WasmEngine: Engine as unknown as WasmEngineModule['WasmEngine'] }));
  const io = vi.fn(() => {
    throw new Error('No transport work during signing setup');
  });
  setRustEngineTransport(() => ({
    now: () => 1000,
    request: io,
    connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
  const options = { engine: 'rust', key: 'owned-public', secret: '', ...input } as ExchangeOptions;
  const exchange = new RustEngineExchange(name, options, new DefaultStore());
  running.push(exchange);
  return { exchange, options, calls, installed, signed, failed, free, io };
};
const ensure = (exchange: RustEngineExchange) => (exchange as unknown as { ensureHost(): Promise<void> }).ensureHost();
const execute = (exchange: RustEngineExchange, work: SignRequest) =>
  (exchange as unknown as { host: { execute(action: unknown): void } }).host.execute({
    op: 'sign',
    attempt: 7,
    work,
    timeoutMs: 1000,
  });

afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
  vi.restoreAllMocks();
});

describe('delegated signer SDK runtime admission', () => {
  it.each(aliases)(
    '%s keeps remote and payload accounts on Rust without retaining callbacks or credentials',
    (name) => {
      for (const callbacks of [
        { remoteSigner: remote() },
        { payloadSigner: payload() },
        { remoteSigner: remote(), payloadSigner: payload() },
      ]) {
        const { exchange, options } = setup(name, { ...callbacks, secret: 'unused-local-sentinel' });
        expect(runsOnRustEngine(name, options)).toBe(true);
        expect(exchange.isPublicDataOnly).toBe(false);
        for (const field of ['key', 'secret', 'remoteSigner', 'payloadSigner', 'localSigner'])
          expect(exchange.options).not.toHaveProperty(field);
        expect(options.secret).toBe('unused-local-sentinel');
        expect(options).toMatchObject(callbacks);
      }
    },
  );

  it.each(aliases)(
    '%s installs the owned provider, selects streaming and preserves raw payload intent when RemoteSigner wins',
    async (name) => {
      const upstream = remote();
      const fallback = payload();
      const r = setup(name, { remoteSigner: upstream, payloadSigner: fallback });
      await ensure(r.exchange);
      expect(r.calls).toHaveLength(1);
      expect(r.calls[0]).toHaveLength(2);
      const config = JSON.parse(r.calls[0][1] as string);
      expect(config).toMatchObject({
        remoteSigning: true,
        payloadSigner: false,
        publicMode: { remoteSigner: true, payloadSigner: true },
        signing: { primary: { credential: expect.any(String), signing: expect.any(String) } },
      });
      expect(config).not.toHaveProperty('secret');
      expect(r.installed).toHaveBeenCalledOnce();
      // The remote wire signs HMAC only, so multi-algorithm commands refuse before release.
      expect(r.installed).toHaveBeenCalledWith('["hmacSha256"]');
      const identity = config.signing.primary;
      const work: SignRequest = {
        receipt: { stampMs: 1000 },
        context: { venue: name, method: 'POST', host: 'fixture.invalid', path: '/private' },
        operations: [
          { kind: 'present', key: identity.credential },
          {
            kind: 'sign',
            key: identity.signing,
            algorithm: 'hmacSha256',
            payload: [{ kind: 'bytes', bytes: Array.from(new TextEncoder().encode('exact-native-preimage')) }],
          },
        ],
      };
      execute(r.exchange, work);
      await vi.waitFor(() => expect(r.signed).toHaveBeenCalledOnce());
      expect(upstream.sign).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'ws-auth',
          exchangeName: name,
          accountName: 'owned-account',
          wsAuth: { toSign: 'exact-native-preimage', outputFormat: 'hex' },
        }),
      );
      expect(fallback).not.toHaveBeenCalled();
      const result = JSON.parse(r.signed.mock.calls[0][1] as string);
      expect(result.values[0]).toEqual(Array.from(new TextEncoder().encode('owned-public')));
      expect(Buffer.from(result.values[1]).toString('hex')).toBe(mac('exact-native-preimage'));
      expect(r.failed).not.toHaveBeenCalled();
      expect(r.io).not.toHaveBeenCalled();
    },
  );

  it.each(aliases)('%s preserves public bypass and does not call a configured provider', (name) => {
    const callback = payload();
    for (const input of [{ publicDataOnly: true }, { key: '' }]) {
      const r = setup(name, { payloadSigner: callback, ...input });
      expect(r.exchange.isPublicDataOnly).toBe(true);
      expect(callback).not.toHaveBeenCalled();
    }
  });

  it('keeps unrelated families closed and rejects malformed admitted providers synchronously', () => {
    expect(runsOnRustEngine('bybit', { engine: 'rust', secret: '', remoteSigner: remote() })).toBe(true);
    expect(runsOnRustEngine('bitunix', { engine: 'rust', secret: '', remoteSigner: remote() })).toBe(true);
    // Unmigrated control family: MEXC (Bitunix gained hosted signing in P7).
    expect(runsOnRustEngine('mexc', { engine: 'rust', secret: '', remoteSigner: remote() })).toBe(false);
    let error: unknown;
    try {
      setup('binance', { remoteSigner: {} as RemoteSigner });
    } catch (failure) {
      error = failure;
    }
    expect(error).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
  });

  it('failed host initialization frees once and retires even a retained signer wrapper', async () => {
    const callback = payload();
    const r = setup('binance', { payloadSigner: callback }, true);
    const retained = (r.exchange as unknown as { signerSetup: { signer: Signer } }).signerSetup.signer;
    await expect(ensure(r.exchange)).rejects.toThrow('capability initialization failed');
    expect(r.free).toHaveBeenCalledOnce();
    await expect(
      retained(
        {
          receipt: {},
          context: { venue: 'binance', method: 'GET', host: 'fixture.invalid', path: '/' },
          operations: [{ kind: 'present', key: 'unowned' }],
        },
        { signal: new AbortController().signal },
      ),
    ).rejects.toThrow(/disposed or canceled/);
    expect(callback).not.toHaveBeenCalled();
    expect(r.io).not.toHaveBeenCalled();
    r.exchange.dispose();
    expect(r.free).toHaveBeenCalledOnce();
  });
});

describe('Bitunix keyless RemoteSigner admission (R1)', () => {
  const doubleSha256 = (toSign: string) => createHash('sha256')
    .update(createHash('sha256').update(toSign).digest('hex') + 'audit-provider').digest('hex');
  const browser = (): RemoteSigner => ({
    accountName: 'owned-account',
    sign: vi.fn(async (request) => ({ id: request.id, wsAuthSignature: doubleSha256(request.wsAuth!.toSign) })),
    isAvailable: () => true, onAvailabilityChange: () => () => {},
  });

  it('constructs on Rust, advertises only sha256SecretSuffix and signs the fused login through the browser', async () => {
    const upstream = browser();
    expect(runsOnRustEngine('bitunix', { engine: 'rust', secret: '', remoteSigner: upstream })).toBe(true);
    const r = setup('bitunix', { remoteSigner: upstream });
    expect(r.exchange.isPublicDataOnly).toBe(false);
    for (const field of ['key', 'secret', 'remoteSigner', 'payloadSigner']) expect(r.exchange.options).not.toHaveProperty(field);
    await ensure(r.exchange);
    expect(JSON.parse(r.calls[0][1] as string)).toMatchObject({ remoteSigning: true, payloadSigner: false });
    expect(r.installed).toHaveBeenCalledWith('["sha256SecretSuffix"]');
    const identity = JSON.parse(r.calls[0][1] as string).signing.primary;
    const prefix = Array.from(new TextEncoder().encode('nonce-abc1000'));
    execute(r.exchange, { receipt: { stampMs: 1000 }, context: { venue: 'bitunix', method: 'WS', host: 'fapi.bitunix.com', path: '/private/' }, operations: [
      { kind: 'sha256', payload: [{ kind: 'bytes', bytes: prefix }, { kind: 'credential', key: identity.credential }, { kind: 'bytes', bytes: [] }] },
      { kind: 'sign', algorithm: 'sha256SecretSuffix', key: identity.signing, payload: [{ kind: 'result', index: 0, encoding: 'hex' }] },
      { kind: 'present', key: identity.credential },
    ] });
    await vi.waitFor(() => expect(r.signed).toHaveBeenCalledOnce());
    expect(upstream.sign).toHaveBeenCalledWith(expect.objectContaining({ type: 'ws-auth', exchangeName: 'bitunix', accountName: 'owned-account',
      wsAuth: { toSign: 'nonce-abc1000owned-public', outputFormat: 'bitunix-double-sha256' } }));
    const result = JSON.parse(r.signed.mock.calls[0][1] as string);
    expect(Buffer.from(result.values[1]).toString('hex')).toBe(doubleSha256('nonce-abc1000owned-public'));
    expect(result.values[2]).toEqual(Array.from(new TextEncoder().encode('owned-public')));
    expect(r.failed).not.toHaveBeenCalled(); expect(r.io).not.toHaveBeenCalled();
  });

  it('still refuses a private PayloadSigner account before any engine work', () => {
    const callback = payload();
    let error: unknown;
    try { setup('bitunix', { payloadSigner: callback }); } catch (failure) { error = failure; }
    expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
    expect(callback).not.toHaveBeenCalled();
  });
});
