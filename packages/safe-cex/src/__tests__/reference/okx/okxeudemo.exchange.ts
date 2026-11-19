import type { ExchangeName, ExchangeOptions } from '../../../types';

import { FrameStore } from '../../../store/frameStore';
import { OKXEUExchange } from './okxeu.exchange';

export class OKXEUDemoExchange extends OKXEUExchange {
  isDemo = true;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: FrameStore) {
    super(exchangeName, { ...opts, testnet: true }, store);
  }
}
