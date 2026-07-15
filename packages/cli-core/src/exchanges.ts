import type { ExchangeInfo } from './types';

/**
 * Supported exchanges with their required fields
 */
export const SUPPORTED_EXCHANGES: Record<string, ExchangeInfo> = {
  bybit: {
    displayName: 'Bybit',
    requiredFields: ['apiKey', 'apiSecret'],
    optionalFields: ['testnet'],
  },
  binance: {
    displayName: 'Binance Futures',
    requiredFields: ['apiKey', 'apiSecret'],
    optionalFields: ['testnet'],
  },
  hyperliquid: {
    displayName: 'Hyperliquid',
    requiredFields: ['privateKey', 'walletAddress'],
    optionalFields: ['testnet'],
  },
  blofin: {
    displayName: 'BloFin',
    requiredFields: ['apiKey', 'apiSecret', 'passphrase'],
    optionalFields: [],
  },
  bitget: {
    displayName: 'Bitget',
    requiredFields: ['apiKey', 'apiSecret', 'passphrase'],
    optionalFields: [],
  },
  woo: {
    displayName: 'WOO X',
    requiredFields: ['apiKey', 'apiSecret'],
    optionalFields: [],
  },
  okx: {
    displayName: 'OKX',
    requiredFields: ['apiKey', 'apiSecret', 'passphrase'],
    optionalFields: ['testnet'],
  },
  bitmex: {
    displayName: 'BitMEX',
    requiredFields: ['apiKey', 'apiSecret'],
    optionalFields: ['testnet'],
  },
  coinbase: {
    displayName: 'Coinbase',
    requiredFields: ['apiKey', 'apiSecret'],
    optionalFields: [],
  },
  kraken: {
    displayName: 'Kraken',
    requiredFields: ['apiKey', 'apiSecret'],
    optionalFields: [],
  },
  kucoin: {
    displayName: 'KuCoin',
    requiredFields: ['apiKey', 'apiSecret', 'passphrase'],
    optionalFields: [],
  },
  phemex: {
    displayName: 'Phemex',
    requiredFields: ['apiKey', 'apiSecret'],
    optionalFields: ['testnet'],
  },
  paradex: {
    displayName: 'Paradex',
    requiredFields: ['privateKey', 'walletAddress'],
    optionalFields: ['testnet'],
  },
  arkham: {
    displayName: 'Arkham',
    requiredFields: ['privateKey', 'walletAddress'],
    optionalFields: [],
  },
};

export function getSupportedExchangeNames(): string[] {
  return Object.keys(SUPPORTED_EXCHANGES);
}

export function getExchangeInfo(name: string): ExchangeInfo | undefined {
  return SUPPORTED_EXCHANGES[name.toLowerCase()];
}
