import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../reference/bitget/bitget.exchange';
import { DefaultStore } from '../../store/store.base';

describe('audit regression: Bitget V2 account information', () => {
  it('maps the V2 spot account-info camelCase response', async () => {
    const exchange = new BitgetExchange(
      'bitget',
      { key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase' },
      new DefaultStore(),
    );
    const get = vi.fn().mockResolvedValue({
      data: { data: { userId: 'sub-42', parentId: 'main-7', inviterId: 'ref-9' } },
    });
    exchange.xhr = { get } as any;

    await expect(exchange.getAccount()).resolves.toEqual({
      userId: 'main-7',
      subId: 'sub-42',
      affiliateId: 'ref-9',
    });
    expect(get).toHaveBeenCalledWith('/api/v2/spot/account/info');
  });
});
