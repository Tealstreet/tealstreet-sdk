import type { Store } from '../../store/store.interface';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { BitmexExchange } from './bitmex.exchange';

export class BitmexTestnetExchange extends BitmexExchange {
  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, { ...opts, testnet: true }, store);
  }
}
