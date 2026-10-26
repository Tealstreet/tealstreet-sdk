import type { Store } from '../../../store/store.interface';
import type { ExchangeName, ExchangeOptions } from '../../../types';

import { OKXEUExchange } from './okxeu.exchange';

export class OKXEUDemoExchange extends OKXEUExchange {
  isDemo = true;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, { ...opts, testnet: true }, store);
  }
}
