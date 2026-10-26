import type { EngineBridgeConstruction } from '@tealstreet/safe-cex-wasm/host';
import { afterEach, expect, it, vi } from 'vitest';
import * as construction from '../../exchanges/rust/construction';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});

it('uses construction policy before startup instead of inferring identity or read credentials from the name', () => {
  const policy: EngineBridgeConstruction = {
    spot: true, demo: true, primaryCredentialsOnly: true,
    payloadSigning: false, apiKeyIndexRequired: false, isolatedBookSubscriptions: false, dispatchSigning:false,
  };
  vi.spyOn(construction, 'deriveBridgeConstruction').mockReturnValue(policy);
  const options = {
    key: 'fixture-primary', secret: 'fixture-primary-secret',
    secondary_key: 'fixture-read', secondary_secret: 'fixture-read-secret',
    secondary_passphrase: 'fixture-read-passphrase', secondaryProxyUrl: 'https://read.invalid',
  };
  const exchange = new RustEngineExchange('binance', options, new DefaultStore());
  running.push(exchange);
  expect(exchange.isSpot).toBe(true);
  expect(exchange.isDemo).toBe(true);
  expect(exchange.hasStarted).toBe(false);
  expect(exchange.engineCapabilities).toBeUndefined();
  expect(exchange.options).toEqual({ key: options.key, secret: options.secret });
  expect(options.secondary_key).toBe('fixture-read');
});

it('applies the declared API-index guard before loading or starting an engine', async () => {
  vi.spyOn(construction, 'deriveBridgeConstruction').mockReturnValue({
    spot: false, demo: false, primaryCredentialsOnly: false,
    payloadSigning: false, apiKeyIndexRequired: true, isolatedBookSubscriptions: false, dispatchSigning:false,
  });
  const exchange = new RustEngineExchange('binance', { key: '', secret: '', apiKeyIndex: '256' }, new DefaultStore());
  running.push(exchange);
  await expect(exchange.validateAccount()).resolves.toEqual({ error: 'Invalid API key index. Expected an integer from 0 to 255.' });
  expect(exchange.hasStarted).toBe(false);
});
