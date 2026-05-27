import qs from 'qs';

import { safeCexAxiosCreate } from '../../safeCexAxiosCreate';
import type { ExchangeOptions } from '../../types';
import { BaseExchange } from '../base';
import { BASE_URL } from './btcc.types';

export const createAPI = (options: ExchangeOptions, exchange: BaseExchange) =>
  safeCexAxiosCreate(
    {
      baseURL: BASE_URL,
      headers: {
        'Content-Type': 'application/json',
      },
      paramsSerializer: {
        serialize: (params) =>
          qs.stringify(params, {
            arrayFormat: 'repeat',
            encodeValuesOnly: false,
          }),
      },
    },
    options,
    exchange,
  );
