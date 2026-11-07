import type { ExchangeName, ExchangeOptions, PayloadSigner } from '@tealstreet/safe-cex/src/types';

import { getExchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';
import { getRegisteredExchangeNames } from '@tealstreet/safe-cex/src/exchanges/registry';
import { assertBetaVenue } from '@tealstreet/safe-cex/src/exchanges/rust/betaVenues';

export type ServerConfig = {
  exchange: ExchangeName;
  options: ExchangeOptions;
};

/**
 * Sends the bytes to sign, base64, in `header`, and `placeholder` as the signature,
 * for a signing proxy that holds the secret to replace.
 */
export const proxySigner =
  (header: string, placeholder: string): PayloadSigner =>
  (request) => {
    if (request.scheme !== 'hmac-sha256') {
      throw new Error(`EXCHANGE_SIGNER_HEADER cannot sign ${request.scheme} payloads yet`);
    }
    return { signature: placeholder, headers: { [header]: Buffer.from(request.payload, 'utf8').toString('base64') } };
  };

const optional = (env: NodeJS.ProcessEnv, name: string): string | undefined => env[name]?.trim() || undefined;

export const readConfig = (env: NodeJS.ProcessEnv): ServerConfig => {
  const exchange = optional(env, 'EXCHANGE');
  const names = getRegisteredExchangeNames();
  if (!exchange) {
    throw new Error(`EXCHANGE must be one of: ${names.join(', ')}`);
  }
  assertBetaVenue(exchange);

  const testnet = optional(env, 'EXCHANGE_TESTNET');
  if (testnet !== undefined && testnet !== 'true' && testnet !== 'false') {
    throw new Error('EXCHANGE_TESTNET must be true or false');
  }

  const engine = optional(env, 'EXCHANGE_ENGINE');
  if (engine !== undefined && engine !== 'rust') {
    throw new Error('EXCHANGE_ENGINE must be rust (or unset for the venue default)');
  }

  const metadata = getExchangeMetadata(exchange as ExchangeName);
  const toggle = (name: string, offered: boolean | undefined): boolean => {
    const value = optional(env, name);
    if (value !== undefined && value !== 'true' && value !== 'false') throw new Error(`${name} must be true or false`);
    if (value === 'true' && !offered) throw new Error(`${name} is not available on ${exchange}`);
    return value === 'true';
  };
  const options =
    toggle('EXCHANGE_OPTIONS', metadata.supportsOptionsToggle || metadata.optionsOnly) || Boolean(metadata.optionsOnly);
  const inverse = toggle('EXCHANGE_INVERSE', metadata.supportsInverseToggle);

  const secret = optional(env, 'EXCHANGE_API_SECRET');
  const header = optional(env, 'EXCHANGE_SIGNER_HEADER');
  const placeholder = optional(env, 'EXCHANGE_SIGNER_SIGNATURE');
  if (!header !== !placeholder) {
    throw new Error('EXCHANGE_SIGNER_HEADER and EXCHANGE_SIGNER_SIGNATURE must be set together');
  }
  if (header && secret) {
    throw new Error('EXCHANGE_API_SECRET and EXCHANGE_SIGNER_HEADER are both set; set one');
  }

  return {
    exchange: exchange as ExchangeName,
    options: {
      ...(engine ? { engine } : {}),
      key: optional(env, 'EXCHANGE_API_KEY') ?? '',
      secret: secret ?? '',
      passphrase: optional(env, 'EXCHANGE_API_PASSPHRASE'),
      testnet: testnet === 'true',
      enableOptionMarkets: options,
      enableInverseMarkets: inverse,
      payloadSigner: header && placeholder ? proxySigner(header, placeholder) : undefined,
      tuning: { consoleLogging: false },
    },
  };
};
