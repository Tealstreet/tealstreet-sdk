import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { exchangeRegistry } from '../registry';

describe('BTCC exchange integration', () => {
  it('registers BTCC in the exchange registry', () => {
    expect(exchangeRegistry.btcc).toBeDefined();
  });

  it('exposes BTCC metadata', () => {
    expect(exchangeMetadata.btcc).toBeDefined();
    expect(exchangeMetadata.btcc.requiresApplicationId).toBe(true);
    expect(exchangeMetadata.btcc.requiresPassphrase).toBe(true);
    expect(exchangeMetadata.btcc.supportsTradesHistory).toBe(true);
    expect(exchangeMetadata.btcc.supportsClosedPnLEndpoint).toBe(true);
  });
});
