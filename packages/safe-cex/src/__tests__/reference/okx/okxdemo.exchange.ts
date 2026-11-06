import type { Store } from '../../../store/store.interface';
import type { ExchangeName, ExchangeOptions } from '../../../types';

import { OKXExchange } from './okx.exchange';

export class OKXDemoExchange extends OKXExchange {
  isDemo = true;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, { ...opts, testnet: true }, store);
  }
}
