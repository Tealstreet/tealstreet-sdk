import { ethers } from 'ethers';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { approveBuilderFeeWithBrowserWallet } from '../../exchanges/hyperliquid/hyperliquid.builderFee';
import { reportEvent } from '../../telemetry';

vi.mock('ethers', async (importOriginal) => {
  const actual = await importOriginal<typeof import('ethers')>();
  return { ...actual, ethers: { ...actual.ethers, BrowserProvider: vi.fn() } };
});
vi.mock('../../telemetry', () => ({ reportEvent: vi.fn() }));

const ADDRESS = '0x1111111111111111111111111111111111111111';
const BUILDER = '0x2222222222222222222222222222222222222222';
const NONCE = 1_750_000_000_000;
const SIGNATURE = `0x${'11'.repeat(32)}${'22'.repeat(32)}1b`;
const CANCELLED = { success: false, message: 'Builder fee approval cancelled-for-owner-change' };
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
type Stage = (typeof stages)[number] | 'submit';

const deferred = () => {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => (resolve = done));
  return { promise, resolve };
};

function approval(
  stage: Stage,
  withHook: boolean | 'signal' | 'callback' = true,
  testnet = false,
  cancelBeforeStart?: 'owner' | 'signal',
) {
  const entered = deferred();
  const release = deferred();
  const calls: string[] = [];
  let currentOwner = 'A';
  const capturedOwner = currentOwner;
  const controller = new AbortController();
  const step = async <T>(name: string, value: T, fails = false): Promise<T> => {
    calls.push(name);
    if (stage === name) {
      entered.resolve();
      await release.promise;
    }
    if (fails) throw new Error('wallet request failed');
    return value;
  };
  let switches = 0;
  const needsAdd = ['switch-failed', 'add', 'add-failed', 'switch-again'].includes(stage);
  const request = vi.fn(async ({ method }: { method: string }) => {
    if (method === 'wallet_addEthereumChain')
      return step(stage === 'add-failed' ? stage : 'add', null, stage === 'add-failed');
    expect(method).toBe('wallet_switchEthereumChain');
    return ++switches === 1 ? step(needsAdd ? 'switch-failed' : 'switch', null, needsAdd) : step('switch-again', null);
  });
  const signer = {
    getAddress: vi.fn(() => step('address', ADDRESS)),
    signTypedData: vi.fn(() => step('sign', SIGNATURE)),
  };
  const provider = {
    send: vi.fn(() => step('accounts', [ADDRESS])),
    getSigner: vi.fn(() => step('signer', signer)),
    destroy: vi.fn(),
  };
  vi.mocked(ethers.BrowserProvider).mockImplementation(function () {
    return provider as unknown as ethers.BrowserProvider;
  });
  vi.stubGlobal('window', { ethereum: { request } });
  const fetch = vi.fn(async () => {
    await step('submit', undefined);
    return { json: async () => ({ status: 'ok' }) };
  });
  vi.stubGlobal('fetch', fetch);
  const assertCurrent = vi.fn(() => {
    if (currentOwner !== capturedOwner) throw new Error('host owner changed');
  });
  if (cancelBeforeStart === 'owner') currentOwner = 'B';
  if (cancelBeforeStart === 'signal') controller.abort();
  const result = approveBuilderFeeWithBrowserWallet(
    ADDRESS,
    testnet,
    BUILDER,
    '0.03%',
    withHook
      ? {
          signal: withHook === 'callback' ? undefined : controller.signal,
          assertCurrent: withHook === 'signal' ? undefined : assertCurrent,
        }
      : undefined,
  );
  return {
    entered,
    release,
    calls,
    request,
    signer,
    provider,
    fetch,
    assertCurrent,
    controller,
    result,
    switchOwner: () => (currentOwner = 'B'),
  };
}

beforeEach(() => {
  vi.spyOn(Date, 'now').mockReturnValue(NONCE);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each(stages)('builder approval deferred at %s', (stage) => {
  it('drops A work after the host switches to B during the wait', async () => {
    const h = approval(stage);
    await h.entered.promise;
    const callsBeforeSwitch = [...h.calls];
    h.switchOwner();
    h.release.resolve();
    expect(await h.result).toEqual(CANCELLED);
    expect(h.calls).toEqual(callsBeforeSwitch);
    expect(h.fetch).not.toHaveBeenCalled();
    expect(reportEvent).not.toHaveBeenCalled();
  });

  it('keeps the same-owner wallet flow and submitted bytes', async () => {
    const h = approval(stage);
    await h.entered.promise;
    h.release.resolve();
    const result = await h.result;
    if (stage === 'add-failed') {
      expect(result.success).toBe(false);
      expect(result.message).toContain('Please manually switch');
      expect(h.fetch).not.toHaveBeenCalled();
      return;
    }
    expect(result).toEqual({ success: true, message: 'Builder fee approved successfully' });
    expect(h.fetch).toHaveBeenCalledExactlyOnceWith('https://api.hyperliquid.xyz/exchange', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        action: {
          type: 'approveBuilderFee',
          hyperliquidChain: 'Mainnet',
          signatureChainId: '0xa4b1',
          builder: BUILDER,
          maxFeeRate: '0.03%',
          nonce: NONCE,
        },
        signature: { r: `0x${'11'.repeat(32)}`, s: `0x${'22'.repeat(32)}`, v: 27 },
        nonce: NONCE,
      }),
    });
    expect(reportEvent).toHaveBeenCalledExactlyOnceWith('hyperliquid.builder-fee-approved', {
      wallet: ADDRESS,
      testnet: false,
    });
  });
});

it('honors a cancellation signal after signing without an owner callback', async () => {
  const h = approval('sign', 'signal');
  await h.entered.promise;
  h.controller.abort();
  h.release.resolve();
  expect(await h.result).toEqual(CANCELLED);
  expect(h.fetch).not.toHaveBeenCalled();
});

it('honors the host callback without a cancellation signal', async () => {
  const h = approval('sign', 'callback');
  await h.entered.promise;
  h.switchOwner();
  h.release.resolve();
  expect(await h.result).toEqual(CANCELLED);
  expect(h.fetch).not.toHaveBeenCalled();
});

it.each(['owner', 'signal'] as const)('refuses an initially cancelled %s before touching the wallet', async (kind) => {
  const h = approval('sign', true, false, kind);
  expect(await h.result).toEqual(CANCELLED);
  expect(h.calls).toEqual([]);
  expect(h.fetch).not.toHaveBeenCalled();
});

it('preserves the wallet rejection outcome for the current owner', async () => {
  const h = approval('accounts');
  await h.entered.promise;
  h.signer.signTypedData.mockRejectedValueOnce(new Error('user rejected the request'));
  h.release.resolve();
  expect(await h.result).toEqual({ success: false, message: 'Approval was rejected by user' });
  expect(h.fetch).not.toHaveBeenCalled();
});

it('still refuses the wrong connected wallet before signing or submitting', async () => {
  const h = approval('accounts');
  await h.entered.promise;
  h.signer.getAddress.mockResolvedValueOnce(BUILDER);
  h.release.resolve();
  const outcome = await h.result;
  expect(outcome.success).toBe(false);
  expect(outcome.message).toContain('Wrong wallet connected');
  expect(h.signer.signTypedData).not.toHaveBeenCalled();
  expect(h.fetch).not.toHaveBeenCalled();
});

it('keeps testnet signing and submission on the configured chain', async () => {
  const h = approval('sign', true, true);
  await h.entered.promise;
  h.release.resolve();
  expect((await h.result).success).toBe(true);
  expect(h.request).toHaveBeenCalledExactlyOnceWith({
    method: 'wallet_switchEthereumChain',
    params: [{ chainId: '0x66eee' }],
  });
  expect(h.signer.signTypedData).toHaveBeenCalledWith(
    {
      name: 'HyperliquidSignTransaction',
      version: '1',
      chainId: 421614,
      verifyingContract: '0x0000000000000000000000000000000000000000',
    },
    {
      'HyperliquidTransaction:ApproveBuilderFee': [
        { name: 'hyperliquidChain', type: 'string' },
        { name: 'maxFeeRate', type: 'string' },
        { name: 'builder', type: 'address' },
        { name: 'nonce', type: 'uint64' },
      ],
    },
    { hyperliquidChain: 'Testnet', maxFeeRate: '0.03%', builder: BUILDER, nonce: NONCE },
  );
  expect(h.fetch).toHaveBeenCalledWith(
    'https://api.hyperliquid-testnet.xyz/exchange',
    expect.objectContaining({
      body: JSON.stringify({
        action: {
          type: 'approveBuilderFee',
          hyperliquidChain: 'Testnet',
          signatureChainId: '0x66eee',
          builder: BUILDER,
          maxFeeRate: '0.03%',
          nonce: NONCE,
        },
        signature: { r: `0x${'11'.repeat(32)}`, s: `0x${'22'.repeat(32)}`, v: 27 },
        nonce: NONCE,
      }),
    }),
  );
});

it('preserves the legacy flow when the optional hook is absent', async () => {
  const h = approval('sign', false);
  await h.entered.promise;
  h.switchOwner();
  h.release.resolve();
  expect((await h.result).success).toBe(true);
  expect(h.fetch).toHaveBeenCalledTimes(1);
  expect(h.assertCurrent).not.toHaveBeenCalled();
});

it('suppresses stale success and telemetry if the owner switches after submission', async () => {
  const h = approval('submit');
  await h.entered.promise;
  h.switchOwner();
  h.release.resolve();
  expect(await h.result).toEqual(CANCELLED);
  expect(h.fetch).toHaveBeenCalledTimes(1);
  expect(reportEvent).not.toHaveBeenCalled();
});
