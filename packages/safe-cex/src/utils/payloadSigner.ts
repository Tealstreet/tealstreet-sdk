import type { ExchangeName, ExchangeOptions, PayloadSignerResult } from '../types';

import createHmac from 'create-hmac';

export const canSignPayload = (options: ExchangeOptions): boolean =>
  Boolean(options.secret || options.payloadSigner);

/** HMAC-SHA256 over `payload`, with the caller's `payloadSigner` when set and the local secret otherwise. */
export const signPayload = async (
  options: ExchangeOptions,
  exchangeName: ExchangeName,
  payload: string,
  encoding: 'hex' | 'base64',
): Promise<PayloadSignerResult> => {
  if (options.payloadSigner) {
    return options.payloadSigner({ exchangeName, key: options.key, scheme: 'hmac-sha256', encoding, payload });
  }
  return { signature: createHmac('sha256', options.secret).update(payload).digest(encoding) };
};
