import { describe, expect, it } from 'vitest';

import { createExchangeAPI } from '../reference/hyperliquid/hyperliquid.api';
import { createWalletFromPrivateKey } from '../../utils/ethereum/createWalletFromPrivateKey';

// Any 32-byte key works; the assertions never depend on the address.
const SECRET = '0x1111111111111111111111111111111111111111111111111111111111111111';

const buildApi = (secret: string) => {
  const exchange = { emitAndLogError: (msg: string) => errors.push(msg) } as any;
  const xhr = createExchangeAPI({ key: 'k', secret, testnet: false } as any, exchange);
  // Capture instead of sending, the same way BaseExchange.signHttpRequest does.
  xhr.defaults.adapter = (config: any) => {
    captured = config;
    return Promise.resolve({ data: {}, status: 200, statusText: 'OK', headers: {}, config });
  };
  return xhr;
};

let captured: any = null;
let errors: string[] = [];

const post = async (secret: string, body: unknown) => {
  captured = null;
  errors = [];
  await buildApi(secret).post('/exchange', body);
  // Interceptors see the object; by adapter time axios has serialized it.
  return typeof captured?.data === 'string' ? JSON.parse(captured.data) : captured?.data;
};

const leverageAction = {
  action: { type: 'updateLeverage', asset: 12, isCross: false, leverage: 10 },
  nonce: 1_700_000_000_000,
};

// The exchange-server runs with secret: '' by design, and hyperliquid signs in
// the method body rather than in an interceptor — so changeMarginType/setLeverage
// threw on createWalletFromPrivateKey('') and never issued a request at all.
// Hosted users could not change margin type or leverage; e2e saw
// "expected 'Cross' to equal 'Isolated'" with zero updateLeverage on the wire.
describe('hyperliquid hosted-mode L1 signing', () => {
  it('throws on an empty secret, which is why the server cannot sign', () => {
    expect(() => createWalletFromPrivateKey('')).toThrow();
  });

  it('leaves the action unsigned when there is no secret', async () => {
    const data = await post('', leverageAction);

    expect(data.signature).toBeUndefined();
    expect(data.action.type).toBe('updateLeverage');
    expect(data.nonce).toBe(leverageAction.nonce);
  });

  it('signs on the replay, where the real secret lives', async () => {
    const data = await post(SECRET, leverageAction);

    expect(data.signature).toMatchObject({
      r: expect.any(String),
      s: expect.any(String),
      v: expect.any(Number),
    });
  });

  it('never re-signs an action that already carries a signature', async () => {
    const existing = { r: '0xdead', s: '0xbeef', v: 27 };

    const data = await post(SECRET, { ...leverageAction, signature: existing });

    expect(data.signature).toEqual(existing);
  });

  it('refuses to sign a fund-moving action and says so', async () => {
    const data = await post(SECRET, {
      action: { type: 'withdraw3', amount: '100', destination: '0xattacker' },
      nonce: 1,
    });

    expect(data.signature).toBeUndefined();
    expect(errors.join(' ')).toContain('withdraw3');
  });

  it('signs over the vault address when one is present', async () => {
    const withVault = await post(SECRET, {
      ...leverageAction,
      vaultAddress: '0x000000000000000000000000000000000000dEaD',
    });
    const withoutVault = await post(SECRET, leverageAction);

    expect(withVault.signature).not.toEqual(withoutVault.signature);
  });

  it('ignores a body that is not an action envelope', async () => {
    const data = await post(SECRET, { type: 'clearinghouseState', user: '0xabc' });

    expect(data.signature).toBeUndefined();
  });
});
