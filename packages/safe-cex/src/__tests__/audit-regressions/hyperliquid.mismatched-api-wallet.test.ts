import { describe, expect, it, vi } from 'vitest';

import { HyperliquidExchange } from '../../exchanges/hyperliquid/hyperliquid.exchange';
import { DefaultStore } from '../../store/store.base';

// Well-known test pair; no value, never used in production.
const KEY = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
const ADDRESS = '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266';

const build = (options: Record<string, unknown>) => {
  const exchange = new HyperliquidExchange('hyperliquid' as any, options as any, new DefaultStore());
  const errors: string[] = [];
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation((msg: any) => {
    errors.push(String(msg));
  });
  return { exchange, errors };
};

const warn = (exchange: HyperliquidExchange, publicOnly = false) =>
  (exchange as any).warnOnMismatchedApiWallet(publicOnly);

// Hyperliquid `/info` reads take the wallet address as a parameter and are
// unauthenticated, so a saved account whose key no longer matches its API wallet
// loads looking healthy and only fails on the first order. validateAccount
// catches it, but only on the add-key form — never for a restored account.
describe('hyperliquid mismatched API wallet warning', () => {
  it('warns when the private key does not derive the saved wallet', () => {
    const { exchange, errors } = build({ key: '0x0000000000000000000000000000000000000001', secret: KEY });

    warn(exchange);

    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain('invalid');
  });

  it('stays quiet for a matching pair', () => {
    const { exchange, errors } = build({ key: ADDRESS, secret: KEY });

    warn(exchange);

    expect(errors).toHaveLength(0);
  });

  it('stays quiet for a wallet-based connection with no key pair', () => {
    const { exchange, errors } = build({ applicationId: ADDRESS });

    warn(exchange);

    expect(errors).toHaveLength(0);
  });

  it('stays quiet in explicit public-data-only mode', () => {
    const { exchange, errors } = build({ key: '0x0000000000000000000000000000000000000001', secret: KEY });

    warn(exchange, true);

    expect(errors).toHaveLength(0);
  });

  it('names the wallet so the user can tell which account is broken', () => {
    const bogus = '0x0000000000000000000000000000000000000001';
    const { exchange, errors } = build({ key: bogus, secret: KEY });

    warn(exchange);

    expect(errors[0]).toContain(bogus);
  });

  it('warns instead of throwing when the secret is malformed', () => {
    // ethers throws on a non-hex secret; startup must survive the diagnostic.
    const { exchange, errors } = build({ key: ADDRESS, secret: '0xsecret' });

    expect(() => warn(exchange)).not.toThrow();
    expect(errors).toHaveLength(1);
  });

  // The e2e bad-api-key spec greps the requests log for an auth-shaped message;
  // a warning it cannot recognise is the same as no warning at all.
  it('emits a message the bad-api-key assertion recognises', () => {
    const { exchange, errors } = build({ key: '0x0000000000000000000000000000000000000001', secret: KEY });

    warn(exchange);

    const authPatterns = [/invalid/i, /auth/i, /api.?key/i, /credential/i, /signature/i];
    expect(authPatterns.some((p) => p.test(errors[0]))).toBe(true);
  });
});
