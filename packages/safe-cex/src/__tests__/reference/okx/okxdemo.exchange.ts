import type { ExchangeName, ExchangeOptions } from '../../../types';

import { FrameStore } from '../../../store/frameStore';
import { OKXExchange } from './okx.exchange';

export class OKXDemoExchange extends OKXExchange {
  isDemo = true;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: FrameStore) {
    super(exchangeName, { ...opts, testnet: true }, store);
  }
}
