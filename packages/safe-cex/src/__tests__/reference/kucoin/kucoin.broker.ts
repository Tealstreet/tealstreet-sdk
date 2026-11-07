import type { AxiosRequestHeaders } from 'axios';
import type { ExchangeOptions } from '../../../types';

import { AxiosHeaders } from 'axios';
import createHmac from 'create-hmac';

type KucoinBrokerConfig = NonNullable<NonNullable<ExchangeOptions['extra']>['kucoinBroker']>;

const trimString = (value: unknown): string | undefined => {
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  return trimmed || undefined;
};

const getHeader = (headers: AxiosHeaders, name: string): string | undefined => {
  const value = headers.get(name);
  return value === null || value === undefined ? undefined : String(value);
};

export const getKucoinRequestTimestamp = (headers?: AxiosRequestHeaders): string | undefined => {
  if (!headers) return undefined;
  return getHeader(new AxiosHeaders(headers), 'KC-API-TIMESTAMP');
};

export const getKucoinBrokerConfig = (options: ExchangeOptions): KucoinBrokerConfig | undefined => {
  const config = options.extra?.kucoinBroker;
  if (!config) return undefined;

  const partner = trimString(config.partner);
  const brokerKey = trimString(config.brokerKey);
  const brokerName = trimString(config.brokerName);
  if (!partner || !brokerKey || !brokerName) return undefined;

  return {
    partner,
    brokerKey,
    brokerName,
    verify: config.verify ?? true,
  };
};

export const addKucoinBrokerHeaders = (
  headers: AxiosHeaders,
  options: ExchangeOptions,
  timestamp: string,
): AxiosHeaders => {
  const config = getKucoinBrokerConfig(options);
  if (!config || !options.key) return headers;

  const partnerSign = createHmac('sha256', config.brokerKey)
    .update(`${timestamp}${config.partner}${options.key}`)
    .digest('base64');

  headers.set('KC-API-TIMESTAMP', timestamp);
  headers.set('KC-API-PARTNER', config.partner);
  headers.set('KC-API-PARTNER-SIGN', partnerSign);
  headers.set('KC-BROKER-NAME', config.brokerName);
  headers.set('KC-API-PARTNER-VERIFY', config.verify ? 'true' : 'false');

  return headers;
};
