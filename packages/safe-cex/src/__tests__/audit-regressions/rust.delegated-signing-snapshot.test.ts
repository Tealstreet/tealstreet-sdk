import type { SignRequest, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, PayloadSigner, RemoteSigner } from '../../types';

import { beforeEach, expect, it, vi } from 'vitest';

import { createExchange } from '../../index';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { snapshotDelegatedSigningOptions } from '../../exchanges/rust/delegatedSigningSnapshot';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';

beforeEach(() => { vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined); });

it('omits absent signer keys from snapshots and retained local-account options', () => {
  for (const options of [{ key: 'fixture-key', secret: 'fixture-secret' },
    { key: 'fixture-key', secret: 'fixture-secret', remoteSigner: undefined, payloadSigner: undefined }]) {
    const snapshot = snapshotDelegatedSigningOptions('binance', options);
    for (const field of ['remoteSigner', 'payloadSigner']) expect(Object.hasOwn(snapshot, field)).toBe(false);
    const exchange = new RustEngineExchange('binance', options, new DefaultStore());
    try {
      for (const field of ['remoteSigner', 'payloadSigner']) expect(Object.hasOwn(exchange.options, field)).toBe(false);
    } finally { exchange.dispose(); }
  }
});

const marker = 'SYNTHETIC_PRIVATE_OUTER_CALLBACK_MARKER';
const fields: Array<'remoteSigner' | 'payloadSigner'> = ['remoteSigner', 'payloadSigner'];
const hostileOptions = fields.flatMap((field) => ['getter', 'get trap', 'has trap'].map((shape) => ({ field, shape })));

it.each(hostileOptions)('redacts an outer $field $shape at constructor and factory entry', async ({ field, shape }) => {
  for (const factory of [false, true]) {
    const input: ExchangeOptions = { key: 'fixture-key', secret: '' };
    const fail = () => { throw new Error(marker); };
    const source: ExchangeOptions = shape === 'getter'
      ? Object.defineProperty(input, field, { enumerable: true, get: fail })
      : new Proxy(input, {
        get(target, property, receiver) {
          if (shape === 'get trap' && property === field) return fail();
          return Reflect.get(target, property, receiver);
        },
        has(target, property) {
          if (shape === 'has trap' && property === field) return fail();
          return Reflect.has(target, property);
        },
      });
    let error: unknown;
    let exchange: Awaited<ReturnType<typeof createExchange>> | undefined;
    try {
      exchange = factory ? await createExchange('binance', source, new DefaultStore())
        : new RustEngineExchange('binance', source, new DefaultStore());
    } catch (caught) { error = caught; }
    try {
      expect.soft(error).toMatchObject({ kind: 'invalidInput', outcome: 'notSent', message: 'Invalid delegated signer setup' });
      expect.soft(String(error)).not.toContain(marker);
      expect.soft(JSON.stringify(error, Object.getOwnPropertyNames(error ?? {}))).not.toContain(marker);
      expect.soft(error).not.toHaveProperty('cause');
      expect.soft(exchange).toBeUndefined();
    } finally { exchange?.dispose(); }
  }
  expect(global.fetch).not.toHaveBeenCalled();
});

// This engine only records the installed opaque identity; no transport or native
// engine is started. The real setup/provider perform the complete signing work.
class IdentityProbe implements WasmEngineLike {
  static primary: { credential: string; signing: string } | undefined;
  constructor(_venue: string, json: string) {
    const options: unknown = JSON.parse(json);
    if (!options || typeof options !== 'object' || !('signing' in options)) throw new Error('Missing signing');
    const signing = options.signing;
    if (!signing || typeof signing !== 'object' || !('primary' in signing)) throw new Error('Missing primary');
    const primary = signing.primary;
    if (!primary || typeof primary !== 'object' || !('credential' in primary) || !('signing' in primary)
      || typeof primary.credential !== 'string' || typeof primary.signing !== 'string') throw new Error('Invalid primary');
    IdentityProbe.primary = { credential: primary.credential, signing: primary.signing };
  }
  capabilities = () => '{}';
  start = () => '[]';
  command = () => '[]';
  takeFrame = () => undefined;
  snapshotFrame = () => new Uint8Array();
  wsOpen = () => '[]';
  wsMessage = () => '[]';
  wsClose = () => '[]';
  wsRouteExhausted = () => '[]';
  httpStart = () => '[]';
  httpChunk = () => '[]';
  httpEnd = () => '[]';
  httpFailed = () => '[]';
  timer = () => '[]';
}

const combinations: Array<'remoteSigner' | 'payloadSigner' | 'both'> = [...fields, 'both'];
const factoryCases = combinations.flatMap((field) => [false, true].map((publicOnly) => ({ field, publicOnly })));

it.each(fields)('admits a public getter-backed $field with ordinary outer fields', async (field) => {
  let reads = 0;
  const sign: RemoteSigner['sign'] = vi.fn();
  const remote: RemoteSigner = { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const payload: PayloadSigner = vi.fn();
  Object.defineProperty(field === 'remoteSigner' ? remote : payload, field === 'remoteSigner' ? 'sign' : 'call', {
    get() {
      if (++reads > 1) throw new Error(marker);
      return field === 'remoteSigner' ? sign : Function.prototype.call;
    },
  });
  const options: ExchangeOptions = { key: 'fixture-key', secret: '' };
  Reflect.set(options, field, field === 'remoteSigner' ? remote : payload);
  const exchange = await createExchange('paradex', options, new DefaultStore());
  try {
    expect(exchange.isPublicDataOnly).toBe(true);
    expect(exchange.privateWebsocket).toBeUndefined();
    expect(reads).toBe(1);
    expect(sign).not.toHaveBeenCalled();
    expect(payload).not.toHaveBeenCalled();
  } finally { exchange.dispose(); }
});

it.each(factoryCases)('snapshots $field once across admission and the factory, public=$publicOnly', async ({ field, publicOnly }) => {
  const reads = { remoteSigner: 0, payloadSigner: 0, sign: 0, isAvailable: 0, onAvailabilityChange: 0, accountName: 0, call: 0 };
  const count = (member: keyof typeof reads) => {
    if (++reads[member] > 1) throw new Error(marker);
  };
  const signed = vi.fn();
  const signature = 'ab'.repeat(32);
  const remote: RemoteSigner = {
    get sign() {
      count('sign');
      return function (this: RemoteSigner, request: Parameters<RemoteSigner['sign']>[0]) {
        expect(this).toBe(remote); signed();
        return Promise.resolve({ id: request.id, wsAuthSignature: signature });
      };
    },
    get isAvailable() { count('isAvailable'); return () => true; },
    get onAvailabilityChange() { count('onAvailabilityChange'); return () => () => {}; },
    get accountName() { count('accountName'); return 'fixture-account'; },
  };
  const payload: PayloadSigner = function () {
    signed();
    return { signature };
  };
  Object.defineProperty(payload, 'call', { get() { count('call'); return Function.prototype.call; } });
  const options: ExchangeOptions = { key: 'fixture-key', secret: '' };
  for (const property of fields) Object.defineProperty(options, property, { enumerable: true, get() {
    count(property);
    if (field !== 'both' && field !== property) return undefined;
    return property === 'remoteSigner' ? remote : payload;
  } });
  const name: ExchangeName = publicOnly ? 'paradex' : 'binance';
  const exchange = await createExchange(name, options, new DefaultStore());
  try {
    expect(exchange.isPublicDataOnly).toBe(publicOnly);
    expect(Object.getOwnPropertySymbols(exchange.options)).toHaveLength(0);
    const expected = { remoteSigner: 1, payloadSigner: 1,
      sign: field === 'payloadSigner' ? 0 : 1, isAvailable: field === 'payloadSigner' ? 0 : 1,
      onAvailabilityChange: field === 'payloadSigner' ? 0 : 1, accountName: field === 'payloadSigner' ? 0 : 1,
      call: field === 'remoteSigner' ? 0 : 1 };
    expect(reads).toEqual(expected);
    const setup: unknown = Reflect.get(exchange, 'signerSetup');
    if (!(setup instanceof DelegatedSignerSetup)) throw new Error('Missing delegated setup');
    if (publicOnly) {
      expect(exchange.privateWebsocket).toBeUndefined();
      expect(setup.primaryComplete).toBe(false);
      expect(signed).not.toHaveBeenCalled();
    } else {
      setup.construct(IdentityProbe, name, '{}');
      const primary = IdentityProbe.primary;
      if (!primary) throw new Error('Missing installed identity');
      const request: SignRequest = { receipt: {}, context: { venue: name, method: 'GET', host: 'fixture.invalid', path: '/' },
        operations: [{ kind: 'sign', key: primary.signing, algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: [65] }] }] };
      for (let attempt = 0; attempt < 2; attempt++)
        await expect(setup.signer(request, { signal: new AbortController().signal })).resolves.toHaveProperty('values');
      expect(signed).toHaveBeenCalledTimes(2);
      expect(reads).toEqual(expected);
    }
  } finally { exchange.dispose(); }
  expect(global.fetch).not.toHaveBeenCalled();
});
