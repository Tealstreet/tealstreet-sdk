import { describe, expect, it } from 'vitest';

import { hmacSha256Base64 } from './reference/apexomni/apexomni.utils';
import { sha256Hex } from './reference/bitunix/bitunix.utils';

// Equivalence oracle for the crypto-js -> @noble/hashes migration.
//
// The expected values below were captured by running the ORIGINAL crypto-js
// implementations against these exact inputs:
//   - bitunix:  cryptojs.SHA256(s).toString(cryptojs.enc.Hex)
//   - apexomni: cryptojs.HmacSHA256(msg, key).toString(cryptojs.enc.Base64)
// They are the ground truth. The @noble/hashes port MUST reproduce them
// byte-for-byte — a mismatch means the signing scheme changed and would break
// live order placement on these exchanges. Do NOT edit the expected strings to
// make a failing port pass.

describe('bitunix sha256Hex (SHA256 -> hex) matches crypto-js', () => {
  it('hashes a simple string', () => {
    expect(sha256Hex('hello')).toBe('2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824');
  });

  it('hashes a realistic pre-sign concatenation', () => {
    expect(sha256Hex('abc123nonce-uuid-1700000000000api-key-XYZsymbolBTCUSDTside BUY')).toBe(
      'abe80aef3c25b6db48b50ba9d89c66ff29d703d87f0fe87ba38a7464d322e7f5',
    );
  });

  it('reproduces the double-SHA256 signing chain (digest then digest+secret)', () => {
    const digest = sha256Hex('uuid-nonce' + '1700000000' + 'API_KEY_ABC' + 'symbolBTCUSDTsideBUY');
    const signature = sha256Hex(digest + 'SECRET_KEY_123');
    expect(signature).toBe('3dd29b7538895a8e2b7534806f5fa964a332cacacd0510b406ff65c1726d2fdc');
  });

  it('hashes the empty string', () => {
    expect(sha256Hex('')).toBe('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
  });
});

describe('apexomni hmacSha256Base64 (HMAC-SHA256 -> base64) matches crypto-js', () => {
  // Key is base64(secret), exactly as apexomni.api.ts derives it.
  const b64Key = (secret: string) => Buffer.from(secret).toString('base64');

  it('signs a GET account message', () => {
    expect(hmacSha256Base64('1700000000GET/api/v3/account', b64Key('mysecret'))).toBe(
      'QBYtV9fPKMSG7Q991xeukNk4hY9oP3gI/0lX3pc6lrA=',
    );
  });

  it('signs a POST order message with an over-block-size secret', () => {
    const msg = '1700000000000POST/api/v3/order' + 'clientId=abc&price=100&side=BUY&size=1&symbol=BTC-USDT';
    const secret = 'aVeryLongSecretKeyThatExceedsBlockSize0123456789abcdefghijklmnopqrstuvwxyz0123456789';
    expect(hmacSha256Base64(msg, b64Key(secret))).toBe('/WNlx0AodaYStSiGZpZa+zUQNFUUhYRnIKnsoCtXiys=');
  });

  it('signs with empty message and empty secret', () => {
    expect(hmacSha256Base64('', b64Key(''))).toBe('thNnmggU2ex3L5XXeMNfxf8Wl8STcVZTxscSFEKSxa0=');
  });
});
