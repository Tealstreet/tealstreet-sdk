import { describe, expect, it } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';

describe('BaseExchange rejectPreflight', () => {
  it('emits the error before throwing', () => {
    const exchange = new BaseExchange('binance', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    const emitted: string[] = [];
    exchange.on('error', (msg: string) => emitted.push(msg));

    expect(() => (exchange as any).rejectPreflight('cannot honor')).toThrow('cannot honor');
    expect(emitted).toEqual(['Exchange Error: cannot honor']);
  });
});
