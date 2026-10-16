import type { AxiosInstance } from 'axios';
import type { ExchangeOptions } from '../../types';
import type { BaseExchange } from '../base';

import { safeCexAxiosCreate } from '../../safeCexAxiosCreate';
import { TXFLOW_MAINNET_REST_URL, TXFLOW_TESTNET_REST_URL } from './txflow.types';

export { TXFLOW_MAINNET_REST_URL, TXFLOW_TESTNET_REST_URL } from './txflow.types';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange): AxiosInstance =>
  safeCexAxiosCreate(
    {
      baseURL: options.testnet ? TXFLOW_TESTNET_REST_URL : TXFLOW_MAINNET_REST_URL,
      headers: {
        'Content-Type': 'application/json',
      },
    },
    options,
    exchange,
  );
