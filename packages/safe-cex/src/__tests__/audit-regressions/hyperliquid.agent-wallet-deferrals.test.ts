import { ethers } from 'ethers';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createAgentWalletWithBrowserWallet } from '../../exchanges/hyperliquid/hyperliquid.agent';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each(['network', 'accounts'])('real ethers agent setup deferred at %s', (stage) => {
  it.each([false, true])('fences internal provider calls when owner changes=%s', async (switchOwner) => {
    let owner = 'A';
    let enter!: () => void;
    let resume!: () => void;
    const entered = new Promise<void>((resolve) => (enter = resolve));
    const pending = new Promise<void>((resolve) => (resume = resolve));
    const address = '0x1111111111111111111111111111111111111111';
    const requests: { method: string; owner: string }[] = [];
    let accountReads = 0;
    const request = async ({ method }: { method: string }) => {
      requests.push({ method, owner });
      if (method === 'wallet_switchEthereumChain') return null;
      if (method === 'eth_chainId') {
        if (stage === 'network') {
          enter();
          await pending;
        }
        return '0xa4b1';
      }
      if (method === 'eth_requestAccounts') return [address];
      if (method === 'eth_accounts') {
        if (++accountReads === 1 && stage === 'accounts') {
          enter();
          await pending;
          return [];
        }
        return [address];
      }
      if (method === 'eth_signTypedData_v4') return `0x${'11'.repeat(32)}${'22'.repeat(32)}1b`;
      throw new Error(`Unexpected synthetic wallet request: ${method}`);
    };
    vi.spyOn(ethers.Wallet, 'createRandom').mockReturnValue(ethers.HDNodeWallet.fromSeed(new Uint8Array(32).fill(3)));
    vi.stubGlobal('window', { ethereum: { request } });
    const fetch = vi.fn(async () => ({ json: async () => ({ status: 'ok' }) }));
    vi.stubGlobal('fetch', fetch);
    const result = createAgentWalletWithBrowserWallet(address, false, undefined, () => {
      if (owner !== 'A') throw new Error('host operation retired');
    });
    await entered;
    if (switchOwner) owner = 'B';
    resume();
    const outcome = await result;
    if (switchOwner) {
      expect(outcome).toEqual({ success: false, message: 'Agent wallet creation cancelled-for-owner-change' });
      expect(requests.filter((row) => row.owner === 'B')).toEqual([]);
      expect(fetch).not.toHaveBeenCalled();
    } else {
      expect(outcome.success).toBe(true);
      expect(requests.filter((row) => row.method === 'eth_signTypedData_v4')).toHaveLength(1);
      expect(fetch).toHaveBeenCalledTimes(1);
    }
  });
});
