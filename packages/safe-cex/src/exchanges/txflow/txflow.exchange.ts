import type { AxiosInstance } from 'axios';
import type { Store } from '../../store/store.interface';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { BaseExchange } from '../base';
import { createAPI } from './txflow.api';
import { TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE } from './txflow.types';

export class TxFlowExchange extends BaseExchange {
  xhr: AxiosInstance;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);
    this.xhr = createAPI(opts, this);
  }

  start = (): Promise<void> => {
    this.rejectPreflight(TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE);
  };
}
