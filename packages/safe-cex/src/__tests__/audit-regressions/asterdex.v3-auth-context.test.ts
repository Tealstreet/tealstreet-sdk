import { describe, expect, it, vi } from 'vitest';

import { AsterDEXExchange } from '../../exchanges/asterdex/asterdex.exchange';
import { DefaultStore } from '../../store/store.base';

describe('audit regression: AsterDEX V3 auth context', () => {
  it('requires the main wallet when API-wallet credentials are entered', async () => {
    const exchange = new AsterDEXExchange(
      'asterdex',
      {
        key: '0x1111111111111111111111111111111111111111',
        secret: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
      },
      new DefaultStore(),
    );
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    await expect(exchange.validateAccount()).rejects.toThrow(
      'Aster API Wallet/vault keys require the Main Wallet Address',
    );
  });
});
