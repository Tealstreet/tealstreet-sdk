import type { ExchangeName, ExchangeOptions, RemoteSigner } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { exchangeRegistry } from '../../exchanges/registry';
import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { runsOnRustEngine, RUST_VENUES, RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { createExchange } from '../../index';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';

const remoteSigner: RemoteSigner = {
  sign: async () => ({ id: 'never-called' }),
  isAvailable: () => true,
  onAvailabilityChange: () => () => {},
};

// exchange-server's hosted shape: the browser holds the secret and signs every request.
const hosted = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: 'hosted-api-key',
  secret: '',
  remoteSigner,
  privateOnly: true,
  engine: 'rust',
  ...extra,
});

describe('Rust-only hosted account selection', () => {
  afterEach(() => vi.restoreAllMocks());
  it('never selects a TypeScript engine for an IN name', () => {
    for (const name of RUST_VENUES) expect(runsOnRustEngine(name, hosted())).toBe(true);
  });
  it('constructs only declared delegated providers, otherwise refuses before I/O', async () => {
    vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined as never);
    for (const name of RUST_VENUES) {
      const options = hosted({ passphrase: 'hosted-passphrase' });
      if (deriveRemoteSigningPolicy(name)?.enabled === true && name !== 'paradex') {
        const exchange = await createExchange(name, options);
        expect(exchange).toBeInstanceOf(RustEngineExchange);
        exchange.dispose();
      } else {
        expect(() => assertBetaAccount(name, options)).toThrow('unsupported in this build');
        await expect(createExchange(name, options)).rejects.toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
      }
    }
  });
  it('web registry loads Rust for every IN name', async () => {
    for (const name of RUST_VENUES) expect(await exchangeRegistry[name]()).toBe(RustEngineExchange);
  });
});
