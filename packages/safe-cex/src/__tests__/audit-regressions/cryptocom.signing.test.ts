import { createHmac } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';

import {
  generateAuthenticatedRequest,
  generateSignature,
  generateWsAuthMessage,
} from '../reference/cryptocom/cryptocom.signing';
import { virtualClock } from '../../utils/virtualClock';

describe('audit regression: Crypto.com request signing', () => {
  it('recursively serializes arrays, objects, and null without JavaScript separators', () => {
    const params = {
      exec_inst: ['REDUCE_ONLY', 'POST_ONLY'],
      order_list: [{ side: 'BUY', quantity: '1' }, { side: 'SELL', quantity: null }],
    };
    const payload =
      'private/create-order-list11api' +
      'exec_instREDUCE_ONLYPOST_ONLYorder_listquantity1sideBUYquantitynullsideSELL' +
      '22';
    const expected = createHmac('sha256', 'secret').update(payload).digest('hex');

    expect(generateSignature('private/create-order-list', 11, 'api', 'secret', params, 22)).toBe(expected);
  });

  it('uses the synchronized virtual clock for REST and WebSocket ids and nonces', () => {
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(1_750_000_000_123);

    expect(generateAuthenticatedRequest('private/user-balance', {}, 'api', 'secret')).toMatchObject({
      id: 1_750_000_000_123,
      nonce: 1_750_000_000_123,
    });
    expect(generateWsAuthMessage('api', 'secret')).toMatchObject({
      id: 1_750_000_000_123,
      nonce: 1_750_000_000_123,
    });
  });
});
