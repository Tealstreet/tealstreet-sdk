import { describe, expect, it } from 'vitest';
import { ethers } from 'ethers';
import { actionHash, agentDigest, signAction, TXFLOW_DOMAIN } from '../reference/txflow/txflow.signing';

// Published throwaway key and payload: https://docs.txflow-testnet.xyz/api/signing.md.
const key = '0x' + 'ab'.repeat(32);
const action = { grouping: 'na', orders: [{ a: 1, b: true, p: '50000.0', s: '0.01', r: false, t: { limit: { tif: 'Gtc' } } }] };
describe('TxFlow official signing vectors', () => {
  it('matches the independent domain, MessagePack hash, digest and signature', async () => {
    expect(ethers.TypedDataEncoder.hashDomain(TXFLOW_DOMAIN)).toBe('0x839dd579adc05b1545a37213db5dd3dc6928532ddf73dfa1b53f647421060a8f');
    const hash = actionHash(action, null, 1700000000000);
    expect(hash).toBe('0x6546d1c3acf430a28a0b2552c66e985f6543646660874734732ae9e9daf4d806');
    expect(agentDigest(hash)).toBe('0xeae1ae63709636fb9567accd33bc5dd2717bfac4d97e9a853f91c7fb391e1cc6');
    expect(await signAction(action, 1700000000000, key)).toEqual({ r: '0x4064848dd881985c52dacdc00f7046f1010f7f370e34620d36fcf3b2d1ecb3e0', s: '0x25c45ad67b9506ac653fc3658f47aa8b6002bfa86a31ae619b334d71aa8b4f6f', v: 27 });
  });
  it('covers exact decimal strings, nonce and vault separation', async () => {
    const baseline = await signAction(action, 7, key);
    expect(await signAction(action, 8, key)).not.toEqual(baseline);
    expect(await signAction({ ...action, orders: [{ ...action.orders[0], p: '50000' }] }, 7, key)).not.toEqual(baseline);
    expect(await signAction(action, 7, key, ethers.ZeroAddress)).not.toEqual(baseline);
    await expect(signAction(action, Number.MAX_SAFE_INTEGER + 1, key)).rejects.toThrow('nonce');
    await expect(signAction(action, 7, key, 'invalid')).rejects.toThrow();
  });
});
