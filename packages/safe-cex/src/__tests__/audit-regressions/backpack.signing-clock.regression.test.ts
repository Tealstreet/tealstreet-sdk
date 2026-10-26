import { afterEach, describe, expect, it, vi } from 'vitest';

import { generateAuthHeaders, generateWsAuthSignature } from '../reference/backpack/backpack.signing';
import { virtualClock } from '../../utils/virtualClock';

const privateKey = Buffer.alloc(32, 7).toString('base64');

describe('Backpack signing clock audit regression', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('uses the synchronized virtual clock for REST authentication', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_723_456_789_012);

    const headers = await generateAuthHeaders('balanceQuery', {}, 'public-key', privateKey);

    expect(headers['X-Timestamp']).toBe('1723456789012');
  });

  it('uses the synchronized virtual clock for private WebSocket authentication', async () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_723_456_789_999);

    const signature = await generateWsAuthSignature('public-key', privateKey);

    expect(signature[2]).toBe('1723456789999');
  });
});
