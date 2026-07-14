import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';

describe('Arkham venue retirement', () => {
  it('is unavailable in every supported environment', () => {
    const arkham = exchangeMetadata.arkham;

    expect(arkham.enabled).toBe(false);
    expect(arkham.enabledInDev).toBe(false);
    expect(arkham.enableInPreview).toBe(false);
  });
});
