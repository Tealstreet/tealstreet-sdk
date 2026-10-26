import { afterEach, describe, expect, it, vi } from 'vitest';

import { ParadexRequestSigner, type ParadexSDKAccount } from './reference/paradex/paradex.sdk';

const createSigner = (
  chainId: string,
  signMessage: (typedData: Record<string, unknown>) => Promise<unknown>,
): ParadexRequestSigner => {
  const sdkAccount = {
    account: {
      address: '0x123',
      signMessage,
    },
    config: {
      paradexChainId: chainId,
    },
    provider: {},
  } as unknown as ParadexSDKAccount;

  return new ParadexRequestSigner(sdkAccount);
};

describe('ParadexRequestSigner.signOrder', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('scales size and price deterministically to avoid float drift', async () => {
    const fixedTimestamp = 1_717_171_717_171;
    vi.spyOn(Date, 'now').mockReturnValue(fixedTimestamp);

    const signMessage = vi.fn().mockResolvedValue([1n, 2n]);
    const chainId = 'PRIVATE_SN_PARACLEAR_MAINNET';
    const signer = createSigner(chainId, signMessage);

    const signed = await signer.signOrder({
      market: 'BTC-USD-PERP',
      side: 'BUY',
      type: 'LIMIT',
      size: '0.00000999',
      price: '0.00000999',
    });

    expect(signed.signature).toBe('["1","2"]');
    expect(signed.signature_timestamp).toBe(fixedTimestamp);
    expect(signMessage).toHaveBeenCalledTimes(1);

    const typedData = signMessage.mock.calls[0][0] as Record<string, unknown>;
    const typedDomain = typedData.domain as Record<string, string>;
    const typedMessage = typedData.message as Record<string, string>;

    expect(typedMessage.timestamp).toBe(String(fixedTimestamp));
    expect(typedMessage.size).toBe('999');
    expect(typedMessage.price).toBe('999');
    expect(typedDomain.chainId).toBe(`0x${Buffer.from(chainId, 'utf8').toString('hex')}`);
  });

  it('preserves hex chain ids and sets market price quantum to zero', async () => {
    const signMessage = vi.fn().mockResolvedValue({ r: '123', s: '456' });
    const chainId = '0x534e5f4d41494e';
    const signer = createSigner(chainId, signMessage);

    await signer.signOrder({
      market: 'ETH-USD-PERP',
      side: 'SELL',
      type: 'MARKET',
      size: '1',
    });

    const typedData = signMessage.mock.calls[0][0] as Record<string, unknown>;
    const typedDomain = typedData.domain as Record<string, string>;
    const typedMessage = typedData.message as Record<string, string>;

    expect(typedDomain.chainId).toBe(chainId);
    expect(typedMessage.price).toBe('0');
    expect(typedMessage.side).toBe('2');
  });
});
