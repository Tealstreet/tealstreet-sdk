import { describe, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { OrderlyExchange } from '../reference/orderly/orderly.exchange';
import { DefaultStore } from '../../store/store.base';

const orderlyFamily = ['woofi', 'kodiakfi', 'mode', 'salsadex', 'what', 'tealstreet-dex'] as const;

describe('audit regression: Orderly 100x leverage support', () => {
  it('clamps account-wide leverage at the venue-supported 100x limit', async () => {
    const exchange = new OrderlyExchange(
      'woofi',
      { key: 'test-key', secret: 'test-secret', applicationId: 'test-account' },
      new DefaultStore(),
    );
    const post = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { post } as any;

    await exchange.setAllLeverage(125);

    expect(post).toHaveBeenCalledWith('/v1/client/leverages', { leverage: 100, margin_mode: 'CROSS' });
  });

  it('offers 100x as a leverage magnet for every Orderly-family integration', () => {
    for (const exchangeName of orderlyFamily) {
      expect(exchangeMetadata[exchangeName].leverageMagnets).toContain(100);
    }
  });
});
