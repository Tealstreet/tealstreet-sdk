import { afterEach, describe, expect, it, vi } from 'vitest';

import { approveBuilderFeeWithBrowserWallet } from '../../exchanges/hyperliquid/hyperliquid.builderFee';

vi.mock('../../telemetry', () => ({ reportEvent: vi.fn() }));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each(['network', 'accounts'])('real ethers defers at %s', (stage) => {
  it.each([false, true])('guards account access when owner switches: %s', async (switchOwner) => {
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
          // A disconnected read makes BrowserProvider.getSigner ask for account access again.
          return [];
        }
        return [address];
      }
      if (method === 'eth_signTypedData_v4') return `0x${'11'.repeat(32)}${'22'.repeat(32)}1b`;
      throw new Error(`Unexpected wallet request: ${method}`);
    };
    vi.stubGlobal('window', { ethereum: { request } });
    const fetch = vi.fn(async () => ({ json: async () => ({ status: 'ok' }) }));
    vi.stubGlobal('fetch', fetch);
    const result = approveBuilderFeeWithBrowserWallet(address, false, undefined, undefined, {
      assertCurrent: () => {
        if (owner !== 'A') throw new Error('owner changed');
      },
    });
    await entered;
    if (switchOwner) owner = 'B';
    resume();
    const outcome = await result;
    if (switchOwner) {
      expect(outcome).toEqual({ success: false, message: 'Builder fee approval cancelled-for-owner-change' });
      expect(requests.filter((row) => row.owner === 'B')).toEqual([]);
      expect(fetch).not.toHaveBeenCalled();
    } else {
      expect(outcome.success).toBe(true);
      expect(requests.filter((row) => row.method === 'eth_requestAccounts')).toHaveLength(stage === 'accounts' ? 2 : 1);
      expect(requests.filter((row) => row.method === 'eth_signTypedData_v4')).toHaveLength(1);
      expect(fetch).toHaveBeenCalledTimes(1);
    }
  });
});
