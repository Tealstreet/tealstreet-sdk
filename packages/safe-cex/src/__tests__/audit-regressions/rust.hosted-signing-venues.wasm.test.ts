import type { ExchangeName, ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { createHmac } from 'node:crypto';

import { getPublicKey, verify } from '@noble/ed25519';
import bs58 from 'bs58';
import { afterEach, describe, expect, it, vi } from 'vitest';

import '../reference/orderly/orderly.encryption';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { createExchange } from '../../index';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';
import { hostedSigningReferenceRegistry } from '../reference/legacyVenueRegistry';

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

describe('OUT hosted accounts are refused while signing references remain available', () => {
  afterEach(() => vi.restoreAllMocks());
  it('rejects every retired hosted factory and direct constructor before starting the clock', async () => {
    const clock = vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined as never);
    for (const name of ['coinbase', 'coinbasespot', ...orderlyNames] as ExchangeName[]) {
      const options: ExchangeOptions = { key: 'hosted-key', secret: '', remoteSigner, privateOnly: true };
      await expect(createExchange(name, options)).rejects.toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
      expect(() => new RustEngineExchange(name, options, new DefaultStore())).toThrow('unsupported in this build');
    }
    expect(clock).not.toHaveBeenCalled();
  });
  it('the retained Orderly reference signs independently verified text-sign vectors', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_760_000_000_000);
    for (const name of orderlyNames) {
      const options = { ...orderlyCredentials() };
      const signing = request(name, 'https://api-evm.orderly.org', '/v1/positions');
      const Venue = await hostedSigningReferenceRegistry[name]!();
      const engine = new Venue(name, options, new DefaultStore());
      const venue = new Venue(name, options, new DefaultStore());
      try {
        const signed = await engine.signHttpRequest(signing);
        expect(signed.error, name).toBeUndefined();
        expect(signed.config?.headers?.['orderly-signature'], name).toBeTruthy();
        expect(signed, name).toEqual(await venue.signHttpRequest(signing));
        // Golden: the signature verifies against the account key over Orderly's text-sign.
        const headers = signed.config!.headers!;
        const seed = bs58.decode(options.secret.split(':')[1]);
        const signature = Buffer.from(headers['orderly-signature'].replace(/-/g, '+').replace(/_/g, '/'), 'base64');
        expect(headers['orderly-textsign'], name).toBe('1760000000000GET/v1/positions');
        expect(verify(signature, new TextEncoder().encode(headers['orderly-textsign']), getPublicKey(seed)), name).toBe(
          true,
        );
      } finally {
        engine.dispose();
        venue.dispose();
      }
    }
  });

  it('the retained Coinbase reference signs independently verified JWT vectors', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_760_000_000_000);
    for (const name of ['coinbase', 'coinbasespot'] as ExchangeName[]) {
      const options = coinbaseCredentials();
      const signing = request(name, 'https://api.coinbase.com', '/api/v3/brokerage/accounts');
      const Venue = await hostedSigningReferenceRegistry[name]!();
      const engine = new Venue(name, options, new DefaultStore());
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
        const seed = Buffer.from(options.secret, 'base64');
        const verifies = (jwt: string) => {
          const [head, body, signature] = jwt.replace(/^Bearer /, '').split('.');
          return verify(
            Buffer.from(signature, 'base64url'),
            new TextEncoder().encode(`${head}.${body}`),
            getPublicKey(seed),
          );
        };
        expect(verifies(header(signed)!), name).toBe(true);
        // The browser's main-thread WS auth signs Coinbase's JWT through the same venue.
        const ws = await engine.signWsAuth({
          id: 'ws-1',
          type: 'ws-auth',
          exchangeName: name,
          wsAuth: { toSign: '', outputFormat: 'coinbase-jwt' },
        });
        expect(ws.error, name).toBeUndefined();
        expect(verifies(ws.wsAuthSignature!), name).toBe(true);
        expect(jwtParts(ws.wsAuthSignature!).payload).toMatchObject({ sub: options.key, iss: 'cdp' });
      } finally {
        engine.dispose();
        venue.dispose();
      }
    }
  });
  it('BloFin hosted HTTP and WS auth sign through the Rust provider with the TS wire', async () => {
    const engine = new RustEngineExchange('blofin', { key: 'k', secret: 's', passphrase: 'p' }, new DefaultStore());
    try {
      const signed = await engine.signHttpRequest(request('blofin', 'https://openapi.blofin.com', '/api/v1/account/balance'));
      expect(signed.error).toBeUndefined();
      const headers = signed.config!.headers!;
      const mac = (text: string) => Buffer.from(createHmac('sha256', 's').update(text).digest('hex')).toString('base64');
      expect(headers['ACCESS-KEY']).toBe('k');
      expect(headers['ACCESS-PASSPHRASE']).toBe('p');
      expect(headers['ACCESS-SIGN']).toBe(
        mac(`/api/v1/account/balanceGET${headers['ACCESS-TIMESTAMP']}${headers['ACCESS-NONCE']}`));
      await expect(
        engine.signWsAuth({ id: 'ws-1', type: 'ws-auth', exchangeName: 'blofin',
          wsAuth: { toSign: '/users/self/verifyGET1nonce', outputFormat: 'hex-base64' } }),
      ).resolves.toEqual({ id: 'ws-1', wsAuthSignature: mac('/users/self/verifyGET1nonce') });
    } finally {
      engine.dispose();
    }
  });
  it('an unsupported signing family refuses without a TS or raw-secret fallback', async () => {
    const engine = new RustEngineExchange('krakenspot', { key: 'k', secret: 's' }, new DefaultStore());
    try {
      await expect(
        engine.signHttpRequest(request('krakenspot', 'https://api.kraken.com', '/0/private/Balance')),
      ).resolves.toEqual({
        id: 'sign-1',
        error: 'Hosted HTTP signing is unsupported in this build',
      });
      await expect(
        engine.signWsAuth({
          id: 'ws-1',
          type: 'ws-auth',
          exchangeName: 'krakenspot',
          wsAuth: { toSign: 'x', outputFormat: 'hex' },
        }),
      ).resolves.toEqual({ id: 'ws-1', error: 'Hosted WS auth signing is unsupported in this build' });
    } finally {
      engine.dispose();
    }
  });
});
