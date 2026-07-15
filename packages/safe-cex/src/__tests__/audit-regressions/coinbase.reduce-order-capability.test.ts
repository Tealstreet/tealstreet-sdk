import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';

describe('Coinbase reduce-order capability audit regression', () => {
  it('enables the shared client-side oversize clamp', () => {
    expect(exchangeMetadata.coinbase.autoDownSizeReduceOrders).toBe(true);
  });
});
