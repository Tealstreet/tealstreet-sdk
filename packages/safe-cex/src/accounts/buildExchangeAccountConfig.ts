import type { ExchangeAccountConfig, ExchangeName } from '../types';

import { exchangeMetadata, getExchangeMetadata } from '../exchangeMetadata';
import { normalizeExchangeAccountConfig } from './normalizeExchangeAccountConfig';

export type ExchangeAccountCredentialField =
  | 'apiKey'
  | 'apiSecret'
  | 'passphrase'
  | 'applicationId'
  | 'apiKeyIndex'
  | 'vault'
  | 'omniKeySeed'
  | 'paradexAddress'
  | 'subkeyAddress'
  | 'subaccount'
  | 'grvtSignerAddress'
  | 'grvtTradingAccountId'
  | 'testnet';

export interface ExchangeAccountCredentialSchema {
  exchange: ExchangeName;
  displayName: string;
  requiredFields: ExchangeAccountCredentialField[];
  optionalFields: ExchangeAccountCredentialField[];
}

export type ExchangeAccountCredentialInput = Partial<ExchangeAccountConfig> & {
  exchange: string;
  name: string;
  privateKey?: string;
  walletAddress?: string;
  secret?: string;
};

export class ExchangeAccountConfigError extends Error {
  readonly missingFields: string[];

  constructor(message: string, missingFields: string[] = []) {
    super(message);
    this.name = 'ExchangeAccountConfigError';
    this.missingFields = missingFields;
  }
}

const trimString = (value: unknown): string | undefined => {
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  return trimmed || undefined;
};

const optionalString = (value: unknown): string | undefined => trimString(value);

const isKnownExchangeName = (exchange: string): exchange is ExchangeName => {
  return exchange in exchangeMetadata;
};

const unique = <T>(values: T[]): T[] => Array.from(new Set(values));

const stripUndefined = <T extends object>(value: T): T => {
  return Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== undefined)) as T;
};

export function getExchangeAccountCredentialSchema(exchange: string): ExchangeAccountCredentialSchema | undefined {
  const normalizedExchange = exchange.toLowerCase();
  if (!isKnownExchangeName(normalizedExchange)) return undefined;

  const metadata = getExchangeMetadata(normalizedExchange);
  const requiredFields: ExchangeAccountCredentialField[] = ['apiKey'];
  const optionalFields: ExchangeAccountCredentialField[] = [];

  if (!metadata.requiresNoSecret) requiredFields.push('apiSecret');
  if (metadata.requiresPassphrase) requiredFields.push('passphrase');
  if (metadata.requiresApplicationId) requiredFields.push('applicationId');
  if (metadata.requiresApiKeyIndex) requiredFields.push('apiKeyIndex');
  if (metadata.requiresOmniKeySeed) requiredFields.push('omniKeySeed');
  if (metadata.requiresParadexAddress) requiredFields.push('paradexAddress', 'subkeyAddress');
  if (normalizedExchange === 'grvt') requiredFields.push('grvtSignerAddress', 'grvtTradingAccountId');

  if (!metadata.requiresPassphrase) optionalFields.push('passphrase');
  if (!metadata.requiresApplicationId) optionalFields.push('applicationId');
  if (!metadata.requiresApiKeyIndex) optionalFields.push('apiKeyIndex');
  if (metadata.supportsVault) optionalFields.push('vault');
  if (metadata.supportsSubaccount) optionalFields.push('subaccount');
  if (metadata.supportsTestnet) optionalFields.push('testnet');

  return {
    exchange: normalizedExchange,
    displayName: metadata.prettyName,
    requiredFields: unique(requiredFields),
    optionalFields: unique(optionalFields),
  };
}

export function getSupportedExchangeAccountCredentialSchemas(): ExchangeAccountCredentialSchema[] {
  return Object.keys(exchangeMetadata)
    .map((exchange) => getExchangeAccountCredentialSchema(exchange))
    .filter((schema): schema is ExchangeAccountCredentialSchema => {
      if (!schema) return false;
      const metadata = getExchangeMetadata(schema.exchange);
      return metadata.enabled && !metadata.hiddenFromNewAccountPicker;
    });
}

export function getSupportedExchangeAccountNames(): ExchangeName[] {
  return getSupportedExchangeAccountCredentialSchemas().map((schema) => schema.exchange);
}

export function buildExchangeAccountConfig(input: ExchangeAccountCredentialInput): ExchangeAccountConfig {
  const { privateKey: _privateKey, walletAddress: _walletAddress, secret: _secret, ...accountInput } = input;
  const exchange = trimString(input.exchange)?.toLowerCase();
  if (!exchange || !isKnownExchangeName(exchange)) {
    throw new ExchangeAccountConfigError(`Unknown exchange: ${input.exchange || ''}`, ['exchange']);
  }

  const name = trimString(input.name);
  if (!name) {
    throw new ExchangeAccountConfigError('Missing required field: name', ['name']);
  }

  const metadata = getExchangeMetadata(exchange);
  const apiKey =
    optionalString(input.apiKey) ||
    (metadata.requiresParadexAddress ? optionalString(input.privateKey) : optionalString(input.walletAddress)) ||
    '';
  const apiSecret =
    optionalString(input.apiSecret) ||
    optionalString(input.secret) ||
    (!metadata.requiresNoSecret ? optionalString(input.privateKey) : undefined) ||
    '';
  const account: ExchangeAccountConfig = {
    ...accountInput,
    exchange,
    name,
    apiKey,
    apiSecret: metadata.requiresNoSecret ? apiSecret || apiKey : apiSecret,
    passphrase: optionalString(input.passphrase),
    secondary_apiKey: optionalString(input.secondary_apiKey),
    secondary_apiSecret: optionalString(input.secondary_apiSecret),
    secondary_passphrase: optionalString(input.secondary_passphrase),
    applicationId: optionalString(input.applicationId),
    apiKeyIndex: optionalString(input.apiKeyIndex),
    vault: optionalString(input.vault),
    omniKeySeed: optionalString(input.omniKeySeed),
    subaccount: optionalString(input.subaccount),
    orderlyExchangeId: optionalString(input.orderlyExchangeId),
    paradexAddress: optionalString(input.paradexAddress) || optionalString(input.walletAddress),
    subkeyAddress: optionalString(input.subkeyAddress),
    grvtSignerAddress: optionalString(input.grvtSignerAddress),
    grvtTradingAccountId: optionalString(input.grvtTradingAccountId),
    user: optionalString(input.user),
    color: optionalString(input.color),
    lastSymbol: optionalString(input.lastSymbol),
    telegramBotToken: optionalString(input.telegramBotToken),
    proxyRegion: optionalString(input.proxyRegion),
  };

  const schema = getExchangeAccountCredentialSchema(exchange);
  const missingFields = ['name', ...(schema?.requiredFields || [])].filter((field) => {
    const value = account[field as keyof ExchangeAccountConfig];
    return typeof value === 'string' ? !value.trim() : !value;
  });

  if (missingFields.length > 0) {
    throw new ExchangeAccountConfigError(
      `Missing required fields for ${metadata.prettyName}: ${missingFields.join(', ')}`,
      missingFields,
    );
  }

  return normalizeExchangeAccountConfig(stripUndefined(account));
}
