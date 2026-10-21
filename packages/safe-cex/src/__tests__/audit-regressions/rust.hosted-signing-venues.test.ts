import type { ExchangeName, ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import bs58 from 'bs58';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { hostedSigningVenues } from '../../exchanges/hostedSigningVenues';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { createExchange } from '../../index';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';

const remoteSigner: RemoteSigner = {
  sign: async () => ({ id: 'never-called' }),
  isAvailable: () => true,
  onAvailabilityChange: () => () => {},
};

const orderlyNames: ExchangeName[] = ['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'];
// Generated per run so no credential-shaped literal is committed.
const orderlyCredentials = () => ({
  key: `ed25519:${bs58.encode(Uint8Array.from({ length: 32 }, (_, index) => index + 1))}`,
  secret: `ed25519:${bs58.encode(Uint8Array.from({ length: 32 }, (_, index) => 32 - index))}`,
  applicationId: '0x' + 'ab'.repeat(32),
});
const coinbaseCredentials = () => ({
  key: 'organizations/hosted-org/apiKeys/hosted-key',
  secret: Buffer.from(Uint8Array.from({ length: 32 }, (_, index) => index * 3)).toString('base64'),
});

const request = (exchangeName: ExchangeName, baseURL: string, url: string): RemoteSignerRequest => ({
  id: 'sign-1',
  type: 'http',
  exchangeName,
  config: { method: 'get', url, baseURL, headers: {} },
});

const jwtParts = (header: string) => {
  const [encodedHeader, encodedPayload] = header.replace(/^Bearer /, '').split('.');
  const decode = (part: string) => JSON.parse(Buffer.from(part, 'base64url').toString('utf8'));
  const { nonce: _nonce, ...head } = decode(encodedHeader);
  return { head, payload: decode(encodedPayload) };
};

describe('hosted Coinbase and Orderly accounts sign as they did on master', () => {
  afterEach(() => vi.restoreAllMocks());

  it('runs every default-Rust name with a kept TS venue on it for a keyless hosted account', async () => {
    vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined as never);
    for (const name of ['coinbase', 'coinbasespot', ...orderlyNames] as ExchangeName[]) {
      const hosted: ExchangeOptions = { key: 'hosted-key', secret: '', remoteSigner, privateOnly: true };
      const exchange = await createExchange(name, hosted);
      try {
        expect(exchange, name).not.toBeInstanceOf(RustEngineExchange);
        expect(exchange, name).toBeInstanceOf(await hostedSigningVenues[name]!());
      } finally {
        exchange.dispose();
      }
      // A locally keyed account on the same name still runs on the engine.
      const local = await createExchange(name, { key: 'k', secret: 's' });
      expect(local, name).toBeInstanceOf(RustEngineExchange);
      local.dispose();
    }
  }, 60_000);

  it('a browser Rust instance signs Orderly requests exactly as the TS venue does', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_760_000_000_000);
    for (const name of orderlyNames) {
      const options = { ...orderlyCredentials() };
      const signing = request(name, 'https://api-evm.orderly.org', '/v1/positions');
      const engine = new RustEngineExchange(name, options, new DefaultStore());
      const Venue = await hostedSigningVenues[name]!();
      const venue = new Venue(name, options, new DefaultStore());
      try {
        const signed = await engine.signHttpRequest(signing);
        expect(signed.error, name).toBeUndefined();
        expect(signed.config?.headers?.['orderly-signature'], name).toBeTruthy();
        expect(signed, name).toEqual(await venue.signHttpRequest(signing));
      } finally {
        engine.dispose();
        venue.dispose();
      }
    }
  });

  it('a browser Rust instance signs Coinbase JWTs as the TS venue does', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_760_000_000_000);
    for (const name of ['coinbase', 'coinbasespot'] as ExchangeName[]) {
      const options = coinbaseCredentials();
      const signing = request(name, 'https://api.coinbase.com', '/api/v3/brokerage/accounts');
      const engine = new RustEngineExchange(name, options, new DefaultStore());
      const Venue = await hostedSigningVenues[name]!();
      const venue = new Venue(name, options, new DefaultStore());
      try {
        const signed = await engine.signHttpRequest(signing);
        const reference = await venue.signHttpRequest(signing);
        expect(signed.error, name).toBeUndefined();
        const header = (response: typeof signed) =>
          response.config?.headers?.['Coinbase-Authorization'] ?? response.config?.headers?.Authorization;
        // The JWT carries a random nonce; everything else must match.
        expect(jwtParts(header(signed)!), name).toEqual(jwtParts(header(reference)!));
        expect(jwtParts(header(signed)!).payload.uri, name).toBe('GET api.coinbase.com/api/v3/brokerage/accounts');
      } finally {
        engine.dispose();
        venue.dispose();
      }
    }
  });

  it('names without a kept TS venue keep the engine answers', async () => {
    const engine = new RustEngineExchange('bybit', { key: 'k', secret: 's' }, new DefaultStore());
    await expect(engine.signHttpRequest(request('bybit', 'https://api.bybit.com', '/v5/order/create'))).resolves.toEqual({
      id: 'sign-1',
      error: 'Exchange does not have xhr for signing',
    });
    await expect(
      engine.signWsAuth({ id: 'ws-1', type: 'ws-auth', exchangeName: 'bybit', wsAuth: { toSign: 'x', outputFormat: 'hex' } }),
    ).resolves.toMatchObject({ id: 'ws-1', wsAuthSignature: expect.any(String) });
    engine.dispose();
  });
});
