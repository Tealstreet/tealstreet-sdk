import {
  getExchangeAccountCredentialSchema,
  getSupportedExchangeAccountCredentialSchemas,
  getSupportedExchangeAccountNames,
  type ExchangeAccountCredentialSchema,
} from '@tealstreet/safe-cex/src/accounts/buildExchangeAccountConfig';

export type ExchangeInfo = ExchangeAccountCredentialSchema;

export function getSupportedExchangeNames(): string[] {
  return getSupportedExchangeAccountNames();
}

export function getExchangeInfo(name: string): ExchangeInfo | undefined {
  return getExchangeAccountCredentialSchema(name);
}

export function getSupportedExchangeInfos(): ExchangeInfo[] {
  return getSupportedExchangeAccountCredentialSchemas();
}
