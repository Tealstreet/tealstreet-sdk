import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions, PayloadSigner, RemoteSigner } from '../../types';

import { expect, it, vi } from 'vitest';

import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const venues: ExchangeName[] = ['coinbase', 'coinbasespot'];
const cases = venues.flatMap(name => ['remote', 'payload'].flatMap(mode =>
  ['absent', 'empty'].flatMap(key => [undefined, false].map(publicDataOnly => ({ name, mode, key, publicDataOnly })))));

function callbacks(mode: string) {
  const remoteSigner = {
    sign: vi.fn<RemoteSigner['sign']>(),
    isAvailable: vi.fn(() => true),
    onAvailabilityChange: vi.fn<RemoteSigner['onAvailabilityChange']>(() => () => {}),
  };
  const payloadSigner = vi.fn<PayloadSigner>();
  return { remoteSigner, payloadSigner, options: mode === 'remote' ? { remoteSigner } : { payloadSigner } };
}

it.each(cases)('constructs public without unused signing authority: %j', async ({ name, mode, key, publicDataOnly }) => {
  const unused = callbacks(mode);
  const options: ExchangeOptions = { key: '', secret: '', publicDataOnly, ...unused.options };
  if (key === 'absent') Reflect.deleteProperty(options, 'key');
  const setup = new DelegatedSignerSetup(name, options);
  try {
    expect(setup.primaryComplete).toBe(false);
    expect(setup.hasSecondary).toBe(false);
    expect(setup.usesPayloadSigner).toBe(false);
    expect(setup.signerAlgorithms).toEqual([]);
    expect(setup.credentialPresence).toEqual({ key: false, secret: false });
    const work: SignRequest = { operations: [], receipt: {}, context: { venue: '', method: '', host: '', path: '' } };
    await expect(setup.signer(work, { signal: new AbortController().signal })).rejects.toMatchObject({
      kind: 'unsupported', outcome: 'notSent',
    });
    const exchange = new RustEngineExchange(name, options, new DefaultStore());
    try {
      expect(exchange.isPublicDataOnly).toBe(true);
      expect(exchange.privateWebsocket).toBeUndefined();
      for (const field of ['key', 'secret', 'remoteSigner', 'payloadSigner']) {
        expect(exchange.options).not.toHaveProperty(field);
      }
    } finally { exchange.dispose(); }
    expect(unused.remoteSigner.sign).not.toHaveBeenCalled();
    expect(unused.remoteSigner.isAvailable).not.toHaveBeenCalled();
    expect(unused.remoteSigner.onAvailabilityChange).not.toHaveBeenCalled();
    expect(unused.payloadSigner).not.toHaveBeenCalled();
  } finally { setup.dispose(); }
});

it.each(venues)('%s preserves keyed private custody and malformed-key refusals', name => {
  for (const mode of ['remote', 'payload']) {
    const unused = callbacks(mode);
    const options: ExchangeOptions = {
      key: ['organizations', 'fixture', 'apiKeys', 'fixture'].join('/'), secret: '', ...unused.options,
    };
    const setup = new DelegatedSignerSetup(name, options);
    try {
      expect(setup.primaryComplete).toBe(true);
      expect(setup.usesPayloadSigner).toBe(mode === 'payload');
      expect(setup.signerAlgorithms).toEqual(['es256', 'ed25519']);
      const exchange = new RustEngineExchange(name, options, new DefaultStore());
      try { expect(exchange.isPublicDataOnly).toBe(false); } finally { exchange.dispose(); }
      expect(() => new DelegatedSignerSetup(name, { ...options, key: 'malformed-key-name' }))
        .toThrow(expect.objectContaining({ kind: 'invalidInput', outcome: 'notSent' }));
      const malformedCallback = { ...options };
      Reflect.set(malformedCallback, mode === 'remote' ? 'remoteSigner' : 'payloadSigner', null);
      expect(() => new DelegatedSignerSetup(name, malformedCallback))
        .toThrow(expect.objectContaining({ kind: 'invalidInput', outcome: 'notSent' }));
      expect(unused.remoteSigner.sign).not.toHaveBeenCalled();
      expect(unused.payloadSigner).not.toHaveBeenCalled();
    } finally { setup.dispose(); }
  }
});
