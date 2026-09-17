import type { ExchangeOptions, RemoteSignerRequest } from '../types';

import { describe, expect, it } from 'vitest';

import { BaseExchange } from '../exchanges/base';
import { DefaultStore } from '../store/store.base';

// Hosted mode signs WS auth on the client and ships the signature to a server
// that holds no secret. Each signature below must match the venue's LOCAL
// (non-remoteSigner) expression byte for byte — when it does not, the venue
// rejects the subscription and the private feed dies silently: no error, no
// fills, while REST polling keeps the UI looking healthy. That is how Kraken,
// blofin and apexomni all broke.
//
// Expected values are GOLDEN VECTORS generated with Node's built-in crypto,
// which is a different implementation from the `create-hmac` package used by
// the code under test. Re-deriving them with the implementation's own
// expression would let a shared misreading of a venue's convention pass.
const SECRET = 'test-secret-value';

class TestExchange extends BaseExchange {
  constructor(options: ExchangeOptions) {
    super('binance', options, new DefaultStore());
  }
}

const sign = (outputFormat: string, toSign: string, secret = SECRET) => {
  const exchange = new TestExchange({ key: 'key', secret });
  return exchange.signWsAuth({
    id: 'req-1',
    type: 'ws-auth',
    exchangeName: 'binance',
    wsAuth: { toSign, outputFormat },
  } as unknown as RemoteSignerRequest);
};

describe('BaseExchange.signWsAuth output formats', () => {
  it('signs hex as bybit/bitmex/woo/phemex do locally', async () => {
    const result = await sign('hex', 'GET/realtime1700000000');
    expect(result.wsAuthSignature).toBe('dda0c91ab082cccc37125f5adc7d4c99902bcdcd938e581b7fb76f38650d4626');
  });

  it('signs base64 as okx/bitget do locally', async () => {
    const result = await sign('base64', '1700000000GET/users/self/verify');
    expect(result.wsAuthSignature).toBe('DCToOka+zCcbsBlRJK8Mq8CG969Due2fJmo/SgdpTgw=');
  });

  it('signs hex-base64 as blofin does: base64 OF THE HEX DIGEST STRING', async () => {
    const result = await sign('hex-base64', '/users/self/verifyGET1700000000abc-nonce');

    expect(result.wsAuthSignature).toBe(
      'YTgyYTdkZDM3N2E1ZWMwZTYxZjUwMGRlNWFlZGVlYjE3NjhjNjllZTAyYjdmNjFhZDIzYzYyMzM4MGU2MjM0Yw==',
    );
    // Decoding must yield hex, not raw digest bytes — the distinction the bug erased.
    expect(Buffer.from(result.wsAuthSignature!, 'base64').toString()).toMatch(/^[0-9a-f]{64}$/);
  });

  it('signs apexomni-base64 keyed by the BASE64 OF THE SECRET', async () => {
    const result = await sign('apexomni-base64', '1700000000GET/ws/accounts');
    expect(result.wsAuthSignature).toBe('7x9rAshqEQ+HDPb4F9rUD1ucPsSesWp7gu6rkIcmEIM=');
  });

  it('signs bitunix-double-sha256 as sha256(sha256(toSign) + secret), NOT an HMAC', async () => {
    // bitunix concatenates the secret rather than keying an HMAC with it. Its
    // ws-private used to sign inline with options.secret, which is '' in hosted
    // mode, so the handshake failed with "double check your key and secret" —
    // visible only against exchange-server, never in local dev.
    const result = await sign('bitunix-double-sha256', 'nonce-abc1700000000api-key-xyz');
    expect(result.wsAuthSignature).toBe('c99d3e027fcb8cbefc05f7a38dfc710011d07e79dbefd8b0bc7b44347bb0e624');
  });

  it('errors instead of mis-signing when kraken-challenge reaches the base signer', async () => {
    const result = await sign('kraken-challenge', 'challenge-token');
    expect(result.wsAuthSignature).toBeUndefined();
    expect(result.error).toContain('KrakenExchange');
  });

  it('errors instead of mis-signing when coinbase-jwt reaches the base signer', async () => {
    const result = await sign('coinbase-jwt', 'coinbase-websocket-jwt.user');
    expect(result.wsAuthSignature).toBeUndefined();
    expect(result.error).toContain('CoinbaseExchange');
  });

  it('errors instead of mis-signing an unknown output format', async () => {
    const result = await sign('some-future-format', 'anything');
    expect(result.wsAuthSignature).toBeUndefined();
    expect(result.error).toContain('Unsupported WS auth outputFormat');
  });

  it('errors when no secret is available', async () => {
    const result = await sign('hex', 'anything', '');
    expect(result.error).toBe('No secret available for WS auth signing');
  });

  it('errors rather than throwing when the request carries no wsAuth payload', async () => {
    const exchange = new TestExchange({ key: 'key', secret: SECRET });
    const result = await exchange.signWsAuth({
      id: 'req-2',
      type: 'ws-auth',
      exchangeName: 'binance',
    } as unknown as RemoteSignerRequest);

    expect(result.wsAuthSignature).toBeUndefined();
    expect(result.error).toBeTruthy();
  });
});
