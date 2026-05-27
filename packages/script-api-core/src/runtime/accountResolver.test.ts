import { describe, expect, it } from 'vitest';

import type { Capabilities } from '../api/capabilities.types';

import { resolveAccount } from './accountResolver';

const capWith = (accounts: readonly string[]): Pick<Capabilities, 'accounts'> => ({ accounts });

describe('resolveAccount (SPEC §2.6)', () => {
  it('explicit opts.account wins when allowlisted', () => {
    expect(resolveAccount({ account: 'a' }, capWith(['a', 'b']), 'b')).toBe('a');
  });

  it('throws when explicit account is not in capabilities', () => {
    expect(() => resolveAccount({ account: 'c' }, capWith(['a', 'b']), 'a')).toThrow(
      "account 'c' not in script capabilities",
    );
  });

  it('falls back to single-capability account when opts.account is absent', () => {
    expect(resolveAccount(undefined, capWith(['only']), 'host-active')).toBe('only');
  });

  it('falls back to active account when capabilities allow multiple', () => {
    expect(resolveAccount({}, capWith(['a', 'b']), 'b')).toBe('b');
  });

  it('validates the resolved active account against capabilities', () => {
    expect(() => resolveAccount({}, capWith(['a', 'b']), 'c')).toThrow("account 'c' not in script capabilities");
  });

  it('throws when no source yields an account', () => {
    expect(() => resolveAccount(undefined, capWith(['a', 'b']), '')).toThrow(
      'no explicit, single-capability, or active account available',
    );
  });

  it('empty allowlist means no restriction — uses the active account as-is', () => {
    expect(resolveAccount(undefined, capWith([]), 'host-active')).toBe('host-active');
  });

  it('empty allowlist still honors an explicit opts.account', () => {
    expect(resolveAccount({ account: 'whatever' }, capWith([]), 'host-active')).toBe('whatever');
  });

  it('empty allowlist still throws when no account source is available', () => {
    expect(() => resolveAccount(undefined, capWith([]), '')).toThrow(
      'no explicit, single-capability, or active account available',
    );
  });
});
