import { ethers } from 'ethers';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createAgentWalletWithBrowserWallet } from '../../exchanges/hyperliquid/hyperliquid.agent';

vi.mock('ethers', async (importOriginal) => {
  const actual = await importOriginal<typeof import('ethers')>();
  return { ...actual, ethers: { ...actual.ethers, BrowserProvider: vi.fn() } };
});

const ADDRESS = '0x1111111111111111111111111111111111111111';
const NONCE = 1_700_000_000_123;
const SIGNATURE = `0x${'11'.repeat(32)}${'22'.repeat(32)}1b`;
// Deterministic synthetic wallet; no live key or account is used.
const AGENT = ethers.HDNodeWallet.fromSeed(new Uint8Array(32).fill(3));
const CANCELLED = { success: false, message: 'Agent wallet creation cancelled-for-owner-change' };
const stages = [
  'switch',
  'switch-failed',
  'add',
  'add-failed',
  'switch-again',
  'accounts',
  'signer',
  'address',
  'sign',
] as const;
type Stage = (typeof stages)[number] | 'submit' | 'reply';
type Retirement = 'owner' | 'profile' | 'logout' | 'unmount' | 'aba';
const deferred = () => {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => (resolve = done));
  return { promise, resolve };
};

function creation(stage: Stage, testnet = false, guarded = true, agentName?: string) {
  const entered = deferred();
  const release = deferred();
  let owner: string | null = 'A';
  let observedOwner: string | null = 'A';
  let revision = 0;
  let mounted = true;
  const capturedRevision = revision;
  const calls: string[] = [];
  const wire: unknown[] = [];
  const step = async <T>(name: string, value: T, fails = false): Promise<T> => {
    calls.push(name);
    if (stage === name) {
      entered.resolve();
      await release.promise;
    }
    if (fails) throw new Error('synthetic wallet refusal');
    return value;
  };
  let switches = 0;
  const needsAdd = ['switch-failed', 'add', 'add-failed', 'switch-again'].includes(stage);
  const request = vi.fn(async ({ method, params }: { method: string; params?: unknown }) => {
    wire.push({ method, params });
    if (method === 'wallet_addEthereumChain')
      return step(stage === 'add-failed' ? stage : 'add', null, stage === 'add-failed');
    expect(method).toBe('wallet_switchEthereumChain');
    return ++switches === 1 ? step(needsAdd ? 'switch-failed' : 'switch', null, needsAdd) : step('switch-again', null);
  });
  const signer = {
    getAddress: vi.fn(() => step('address', ADDRESS)),
    signTypedData: vi.fn((...args: unknown[]) => {
      wire.push({ method: 'signTypedData', args });
      return step('sign', SIGNATURE);
    }),
  };
  const provider = {
    send: vi.fn((...args: unknown[]) => {
      wire.push({ method: 'provider.send', args });
      return step('accounts', [ADDRESS]);
    }),
    getSigner: vi.fn(() => step('signer', signer)),
    destroy: vi.fn(),
  };
  vi.mocked(ethers.BrowserProvider).mockImplementation(function () {
    return provider as unknown as ethers.BrowserProvider;
  });
  vi.stubGlobal('window', { ethereum: { request } });
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    wire.push({ url, ...init });
    await step('submit', undefined);
    return { json: () => step('reply', { status: 'ok' }) };
  });
  vi.stubGlobal('fetch', fetch);
  const assertCurrent = vi.fn(() => {
    if (!mounted || owner !== 'A' || observedOwner !== 'A' || revision !== capturedRevision)
      throw new Error('host operation retired');
  });
  const result = createAgentWalletWithBrowserWallet(ADDRESS, testnet, agentName, guarded ? assertCurrent : undefined);
  return {
    entered,
    release,
    calls,
    wire,
    request,
    signer,
    provider,
    fetch,
    assertCurrent,
    result,
    retire(change: Retirement) {
      if (change === 'owner') {
        owner = 'B';
        observedOwner = 'B';
        revision++;
      }
      if (change === 'profile') {
        observedOwner = 'B';
        revision++;
      }
      if (change === 'logout') {
        owner = null;
        observedOwner = null;
        revision++;
      }
      if (change === 'unmount') mounted = false;
      if (change === 'aba') {
        owner = 'A';
        observedOwner = 'A';
        revision += 2;
      }
    },
  };
}

beforeEach(() => {
  vi.spyOn(Date, 'now').mockReturnValue(NONCE);
  vi.spyOn(Math, 'random').mockReturnValue(0.25);
  vi.spyOn(ethers.Wallet, 'createRandom').mockReturnValue(AGENT);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each([false, true])('agent creation testnet=%s', (testnet) => {
  it.each(['owner', 'profile', 'logout', 'unmount', 'aba'] as const)(
    'sends nothing after %s during signing',
    async (change) => {
      const h = creation('sign', testnet);
      await h.entered.promise;
      h.retire(change);
      h.release.resolve();
      expect(await h.result).toEqual(CANCELLED);
      expect(h.fetch).not.toHaveBeenCalled();
    },
  );
  describe.each(stages)('deferred at %s', (stage) => {
    it('abandons all later wallet requests and approval on owner replacement', async () => {
      const h = creation(stage, testnet);
      await h.entered.promise;
      const before = [...h.calls];
      h.retire('owner');
      h.release.resolve();
      expect(await h.result).toEqual(CANCELLED);
      expect(h.calls).toEqual(before);
      expect(h.fetch).not.toHaveBeenCalled();
    });
    it('preserves same-owner legacy wallet, signature and submission bytes', async () => {
      const guarded = creation(stage, testnet, true, 'Custom agent');
      await guarded.entered.promise;
      guarded.release.resolve();
      const guardedResult = await guarded.result;
      const legacy = creation(stage, testnet, false, 'Custom agent');
      await legacy.entered.promise;
      legacy.release.resolve();
      expect(await legacy.result).toEqual(guardedResult);
      expect(JSON.stringify(guarded.wire)).toBe(JSON.stringify(legacy.wire));
      if (stage === 'add-failed') {
        expect(guardedResult.success).toBe(false);
        expect(guarded.fetch).not.toHaveBeenCalled();
      } else {
        expect(guardedResult).toEqual({
          success: true,
          message: 'Agent wallet created successfully',
          agentAddress: AGENT.address,
          agentPrivateKey: AGENT.privateKey,
        });
        expect(guarded.fetch).toHaveBeenCalledExactlyOnceWith(
          testnet ? 'https://api.hyperliquid-testnet.xyz/exchange' : 'https://api.hyperliquid.xyz/exchange',
          {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              action: {
                type: 'approveAgent',
                hyperliquidChain: testnet ? 'Testnet' : 'Mainnet',
                signatureChainId: testnet ? '0x66eee' : '0xa4b1',
                agentAddress: AGENT.address,
                agentName: 'Custom agent',
                nonce: NONCE,
              },
              signature: { r: `0x${'11'.repeat(32)}`, s: `0x${'22'.repeat(32)}`, v: 27 },
              nonce: NONCE,
            }),
          },
        );
      }
    });
  });
  it.each(['submit', 'reply'] as const)('does not expose an agent key after retirement during %s', async (stage) => {
    const h = creation(stage, testnet);
    await h.entered.promise;
    h.retire('owner');
    h.release.resolve();
    expect(await h.result).toEqual(CANCELLED);
    expect(h.fetch).toHaveBeenCalledTimes(1);
  });
  it('keeps the optional third argument and generated default agent name', async () => {
    const h = creation('sign', testnet);
    await h.entered.promise;
    h.release.resolve();
    expect((await h.result).success).toBe(true);
    expect(h.fetch.mock.calls[0][1]?.body).toContain('"agentName":"Tealstreet jjjj"');
  });
});
