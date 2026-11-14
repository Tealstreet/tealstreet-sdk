import type { ExchangeName, ExchangeOptions, RemoteSigner } from '../../types';

import { expect, it, vi } from 'vitest';

import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

it('backpack_public_init_never_installs_signer_or_opens_private_ws', () => {
  const sign = vi.fn<RemoteSigner['sign']>();
  const available = vi.fn<RemoteSigner['isAvailable']>(() => true);
  const subscribe = vi.fn<RemoteSigner['onAvailabilityChange']>(() => () => {});
  const remoteSigner: RemoteSigner = { sign, isAvailable: available, onAvailabilityChange: subscribe };
  const payloadSigner = vi.fn<NonNullable<ExchangeOptions['payloadSigner']>>();
  const inputs: ExchangeOptions[] = [
    { key: 'unused', secret: '', publicDataOnly: true, remoteSigner },
    { key: '', secret: '', remoteSigner },

    { key: '', secret: '', publicDataOnly: false, remoteSigner },
    { key: '', secret: '', payloadSigner },

  ];
  for (const field of ['remoteSigner', 'payloadSigner']) {
    for (const callback of [null, false, 'unused-invalid-callback']) {
      const options: ExchangeOptions = { key: '', secret: '' };
      Reflect.set(options, field, callback);
      inputs.push(options);
    }
  }
  const missing: ExchangeOptions = { key: '', secret: '', remoteSigner };
  Reflect.deleteProperty(missing, 'key');
  inputs.push(missing);
  for (const options of inputs) {
    const setup = new DelegatedSignerSetup('backpack', options);
    try {
      expect(setup.intent).toEqual({ remoteSigner: options.remoteSigner !== undefined, payloadSigner: options.payloadSigner !== undefined });
      expect(setup.primaryComplete).toBe(false);
      expect(setup.hasSecondary).toBe(false);
      expect(setup.credentialPresence).toEqual({ key: false, secret: false });
      expect(setup.signerAlgorithms).toEqual([]);
    } finally { setup.dispose(); }
    const exchange = new RustEngineExchange('backpack', options, new DefaultStore());
    try {
      expect(exchange.isPublicDataOnly).toBe(true);
      expect(exchange.privateWebsocket).toBeUndefined();
      for (const field of ['key', 'secret', 'remoteSigner', 'payloadSigner']) expect(exchange.options).not.toHaveProperty(field);
    } finally { exchange.dispose(); }
  }
  expect(sign).not.toHaveBeenCalled();
  expect(available).not.toHaveBeenCalled();
  expect(subscribe).not.toHaveBeenCalled();
  expect(payloadSigner).not.toHaveBeenCalled();
});

it('backpack_private_payload_and_undeclared_venue_stay_refused', () => {
  const venues: ExchangeName[] = ['backpack', 'weex', 'krakenspot'];
  for (const name of venues) {
    const create = () => new RustEngineExchange(name, { key: 'fixture-key', secret: '', payloadSigner: vi.fn() }, new DefaultStore());
    expect(create).toThrow();
    try { create(); } catch (error) {
      expect(error).toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
      if (name === 'backpack') expect(error).toHaveProperty('message', 'Backpack requires RemoteSigner Ed25519 custody');
      if (name === 'krakenspot') expect(error).toHaveProperty('message', expect.stringContaining('delegated signing is unsupported in this build'));
    }
  }
});
