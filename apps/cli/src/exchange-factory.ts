import { createExchangeFromAccount } from '../../../packages/safe-cex/src/createExchangeFromAccount';
import type { BaseExchange } from '../../../packages/safe-cex/src/exchanges/base';
import { ensureAwsProxyMigrationFlag } from './aws-proxy-migration';
import type { AccountConfig } from './config';

// Supported exchanges with their required fields
export const SUPPORTED_EXCHANGES: Record<
  string,
  {
    displayName: string;
    requiredFields: ('apiKey' | 'apiSecret' | 'passphrase' | 'privateKey' | 'walletAddress')[];
    optionalFields: ('testnet' | 'passphrase')[];
  }
> = {
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
  okxeu: {
    displayName: 'OKX EU',
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
  dydx: {
    displayName: 'dYdX',
    requiredFields: ['privateKey', 'walletAddress'],
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

export function getExchangeInfo(name: string) {
  return SUPPORTED_EXCHANGES[name.toLowerCase()];
}

/**
 * Build a `BaseExchange` from a CLI account config. This file's
 * `SUPPORTED_EXCHANGES` map is unused by other apps/cli modules (only its own
 * `getSupportedExchangeNames` / `getExchangeInfo` helpers reach it, and those
 * have no external consumers — `apps/cli/src/index.ts` carries its own
 * duplicate map for the Commander-level UX). The exchange create/start/stop
 * helpers below are also unconsumed today; kept in place because the file is
 * load-bearing for callers that might surface in the embedded-CLI / serve
 * paths. Marked as dead-but-preserved per root CLAUDE.md ("Mention unrelated
 * dead code — don't delete it").
 *
 * Implementation note: the original handwritten `ExchangeOptions` mapping was
 * incomplete (it carried `privateKey` / `walletAddress` top-level fields that
 * never existed in the canonical `ExchangeOptions` shape). Replaced with the
 * canonical `createExchangeFromAccount` helper from safe-cex so wallet-based
 * exchanges (Hyperliquid, dYdX, Paradex, Arkham) get their auth fields routed
 * through the same translation layer the web app uses.
 */
export async function createExchangeFromConfig(account: AccountConfig): Promise<BaseExchange> {
  // Resolve the AWS->Vultr proxy-migration gate (once per process) before any exchange
  // is built, so safe-cex picks the right public CORS host before virtualClock/REST start.
  await ensureAwsProxyMigrationFlag();
  return createExchangeFromAccount(account);
}

export async function startExchange(exchange: BaseExchange): Promise<void> {
  await exchange.start();
}

export async function stopExchange(exchange: BaseExchange): Promise<void> {
  // `BaseExchange` exposes lifecycle teardown as `dispose()`; the previously
  // referenced `.stop()` was a phantom of an earlier API.
  exchange.dispose();
}
