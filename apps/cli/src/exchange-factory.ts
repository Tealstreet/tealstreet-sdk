import { createExchangeFromAccount } from '../../../packages/safe-cex/src/createExchangeFromAccount';
import type { BaseExchange } from '../../../packages/safe-cex/src/exchanges/base';
import {
  getExchangeAccountCredentialSchema,
  getSupportedExchangeAccountNames,
} from '../../../packages/safe-cex/src/accounts/buildExchangeAccountConfig';
import { ensureAwsProxyMigrationFlag } from './aws-proxy-migration';
import type { AccountConfig } from './config';

export function getSupportedExchangeNames(): string[] {
  return getSupportedExchangeAccountNames();
}

export function getExchangeInfo(name: string) {
  return getExchangeAccountCredentialSchema(name);
}

/**
 * Build a `BaseExchange` from a CLI account config using the same account →
 * exchange-options conversion used by web and every other runtime.
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
